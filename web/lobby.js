// Browser side of the lobby: a presence socket to the signalling service, and one WebRTC data
// channel to every other player (a full mesh, at most 16 players). Chat and challenges travel
// over those channels, never through the server, which only learns who is online and relays the
// handshakes between players.

import {
  cleanChat,
  createRateLimiter,
  displayName,
  parsePeerMessage,
  randomId,
  shouldOffer,
} from './lobby-protocol.js';

const STUN = [{ urls: 'stun:stun.cloudflare.com:3478' }];
const PEER_TIMEOUT_MS = 20_000;
const PEER_RETRY_MS = 2000;
const RECONNECT_MS = [1000, 2000, 4000, 8000, 15000];
const SOCKET_OPEN = 1;

export class Lobby {
  /**
   * @param {string} signalingUrl base URL of the signalling service
   * @param {string} nick already checked with cleanNick
   * @param {{onState?: (s: {phase: string}) => void, onRoster?: (players: object[]) => void,
   *          onChat?: (c: {from: {id: string, name: string}, text: string, mine: boolean}) => void,
   *          onPeerMessage?: (id: string, msg: object) => void,
   *          onPeerLeft?: (id: string) => void}} handlers
   * @param {object} [deps] browser APIs, overridable for tests
   */
  constructor(signalingUrl, nick, handlers, deps = {}) {
    this.base = signalingUrl.replace(/\/+$/, '');
    this.nick = nick;
    this.h = handlers;
    this.WS = deps.WebSocket ?? WebSocket;
    this.PC = deps.RTCPeerConnection ?? RTCPeerConnection;
    this.setTimer = deps.setTimeout ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = deps.clearTimeout ?? ((t) => clearTimeout(t));
    this.now = deps.now ?? (() => performance.now());
    this.newId = deps.randomId ?? randomId;
    this.myId = null;
    this.myStatus = 'lobby';
    this.ws = null;
    this.players = new Map(); // id -> {id, nick, status, link}
    this.peers = new Map(); // id -> {pc, dc, pendingIce, timer, retried, limiter}
    this.muted = new Set();
    this.closed = false;
    this.noRetry = false;
    this.attempt = 0;
    this.reconnectTimer = null;
    this.sendLimiter = createRateLimiter({ perSecond: 2, burst: 2, now: this.now });
  }

  /** Open the presence socket. Every (re)connection uses a fresh id, so a stale socket can
   *  never clash with the new one and nobody can take over someone else's id. */
  connect() {
    this.closed = false;
    this.noRetry = false;
    const old = this.ws;
    if (old) {
      // A replaced socket must never deliver anything or trigger a reconnect again.
      old.onopen = old.onmessage = old.onclose = null;
      try { old.close(); } catch { /* already closed */ }
    }
    const myId = this.newId();
    this.myId = myId;
    this.players.clear();
    const ws = (this.ws = new this.WS(`${this.base.replace(/^http/, 'ws')}/lobby/ws`));
    ws.onopen = () => ws.send(JSON.stringify({ type: 'hello', id: myId, nick: this.nick }));
    ws.onmessage = (e) => this.#onServer(e.data);
    ws.onclose = () => this.#onSocketClosed(ws);
    ws.onerror = () => {}; // a close always follows
    this.h.onState?.({ phase: this.attempt ? 'reconnecting' : 'connecting' });
  }

  /** Send chat to everyone connected. Returns 'ok', 'empty' or 'rate'. */
  chat(text) {
    const clean = cleanChat(text);
    if (clean === null) return 'empty';
    if (!this.sendLimiter.take()) return 'rate';
    const payload = JSON.stringify({ type: 'chat', text: clean });
    for (const peer of this.peers.values()) {
      if (peer.dc.readyState === 'open') peer.dc.send(payload);
    }
    this.h.onChat?.({ from: { id: this.myId, name: displayName(this.nick, this.myId) }, text: clean, mine: true });
    return 'ok';
  }

  /** Send a message to one player over their data channel; false if not connected. */
  sendTo(id, msg) {
    const peer = this.peers.get(id);
    if (!peer || peer.dc.readyState !== 'open') return false;
    peer.dc.send(JSON.stringify(msg));
    return true;
  }

  setStatus(status) {
    this.myStatus = status;
    if (this.ws?.readyState === SOCKET_OPEN) this.ws.send(JSON.stringify({ type: 'status', status }));
    this.#emitRoster();
  }

  mute(id, on) {
    if (on) this.muted.add(id);
    else this.muted.delete(id);
    this.#emitRoster();
  }

  /** Try the presence socket again now instead of waiting for the backoff. */
  retryNow() {
    if (this.closed) return;
    this.clearTimer(this.reconnectTimer);
    this.reconnectTimer = null;
    if (!this.ws || this.ws.readyState !== SOCKET_OPEN) this.connect();
  }

  close() {
    this.closed = true;
    this.clearTimer(this.reconnectTimer);
    for (const peer of this.peers.values()) this.#closePeer(peer);
    this.peers.clear();
    this.players.clear();
    try { this.ws?.close(); } catch { /* already closed */ }
  }

  // ---- presence socket ---------------------------------------------------------------

  #onServer(raw) {
    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    switch (msg?.type) {
      case 'welcome':
        this.attempt = 0;
        for (const p of Array.isArray(msg.roster) ? msg.roster : []) {
          if (p.id !== this.myId) this.#addPlayer(p);
        }
        this.h.onState?.({ phase: 'online' });
        this.#emitRoster();
        break;
      case 'joined':
        this.#addPlayer(msg.peer);
        this.#emitRoster();
        break;
      case 'left':
        this.#removePlayer(msg.id);
        this.#emitRoster();
        break;
      case 'status': {
        const p = this.players.get(msg.id);
        if (p) {
          p.status = msg.status === 'in-match' ? 'in-match' : 'lobby';
          this.#emitRoster();
        }
        break;
      }
      case 'signal':
        this.#onSignal(msg.from, msg.data).catch((err) => console.warn('[ntba-lobby] signal failed', err));
        break;
      case 'full':
        this.noRetry = true;
        this.h.onState?.({ phase: 'full' });
        break;
      case 'error':
        console.warn('[ntba-lobby] server error', msg.reason);
        break;
    }
  }

  #onSocketClosed(ws) {
    if (ws !== this.ws) return; // a stale socket
    for (const peer of this.peers.values()) this.#closePeer(peer);
    this.peers.clear();
    const dropped = [...this.players.keys()];
    this.players.clear();
    this.#emitRoster();
    for (const id of dropped) this.h.onPeerLeft?.(id);
    if (this.closed || this.noRetry) return;
    this.h.onState?.({ phase: 'reconnecting' });
    const delay = RECONNECT_MS[Math.min(this.attempt, RECONNECT_MS.length - 1)];
    this.attempt += 1;
    this.reconnectTimer = this.setTimer(() => {
      if (!this.closed) this.connect();
    }, delay);
  }

  #signal(to, data) {
    if (this.ws?.readyState === SOCKET_OPEN) this.ws.send(JSON.stringify({ type: 'signal', to, data }));
  }

  // ---- players and the mesh ----------------------------------------------------------

  #addPlayer(p) {
    if (!p || typeof p.id !== 'string' || this.players.has(p.id)) return;
    this.players.set(p.id, {
      id: p.id,
      nick: String(p.nick),
      status: p.status === 'in-match' ? 'in-match' : 'lobby',
      link: 'connecting',
    });
    this.#startPeer(p.id, false);
  }

  #removePlayer(id) {
    this.players.delete(id);
    const peer = this.peers.get(id);
    if (peer) {
      this.clearTimer(peer.timer);
      this.#closePeer(peer);
      this.peers.delete(id);
    }
    this.h.onPeerLeft?.(id);
  }

  #startPeer(id, retried) {
    const pc = new this.PC({ iceServers: STUN });
    const dc = pc.createDataChannel('lobby', { negotiated: true, id: 0 });
    const peer = {
      pc,
      dc,
      pendingIce: [],
      timer: null,
      retried,
      limiter: createRateLimiter({ perSecond: 5, burst: 10, now: this.now }),
    };
    this.peers.set(id, peer);
    dc.onopen = () => {
      this.clearTimer(peer.timer);
      this.#setLink(id, 'open');
    };
    dc.onclose = () => this.#fail(id, peer);
    dc.onmessage = (e) => this.#onPeerMessage(id, e.data);
    pc.onicecandidate = (e) => {
      if (e.candidate) this.#signal(id, { kind: 'ice', candidate: e.candidate.toJSON() });
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'failed') this.#fail(id, peer);
    };
    peer.timer = this.setTimer(() => this.#fail(id, peer), PEER_TIMEOUT_MS);
    if (shouldOffer(this.myId, id)) {
      (async () => {
        await pc.setLocalDescription(await pc.createOffer());
        this.#signal(id, { kind: 'offer', sdp: pc.localDescription.sdp });
      })().catch(() => this.#fail(id, peer));
    }
    return peer;
  }

  #fail(id, peer) {
    if (this.peers.get(id) !== peer) return;
    this.clearTimer(peer.timer);
    this.#closePeer(peer);
    this.peers.delete(id);
    this.#setLink(id, 'unreachable');
    // Only the side that makes the offers retries, and only once.
    if (!peer.retried && shouldOffer(this.myId, id)) {
      this.setTimer(() => {
        if (this.closed || !this.players.has(id) || this.peers.has(id)) return;
        this.#setLink(id, 'connecting');
        this.#startPeer(id, true);
      }, PEER_RETRY_MS);
    }
  }

  #closePeer(peer) {
    try {
      peer.dc.onclose = null;
      peer.dc.close();
      peer.pc.close();
    } catch {
      // Already closed.
    }
  }

  async #onSignal(from, data) {
    if (typeof from !== 'string' || !this.players.has(from) || data === null || typeof data !== 'object') return;
    if (data.kind === 'offer') {
      if (shouldOffer(this.myId, from)) return; // we make the offers for this pair; ignore a stray one
      const old = this.peers.get(from);
      if (old) {
        this.clearTimer(old.timer);
        this.#closePeer(old);
        this.peers.delete(from);
      }
      this.#setLink(from, 'connecting');
      const peer = this.#startPeer(from, true);
      await peer.pc.setRemoteDescription({ type: 'offer', sdp: data.sdp });
      await peer.pc.setLocalDescription(await peer.pc.createAnswer());
      this.#signal(from, { kind: 'answer', sdp: peer.pc.localDescription.sdp });
      await this.#flushIce(peer);
      return;
    }
    const peer = this.peers.get(from);
    if (!peer) return;
    if (data.kind === 'answer') {
      await peer.pc.setRemoteDescription({ type: 'answer', sdp: data.sdp });
      await this.#flushIce(peer);
    } else if (data.kind === 'ice') {
      if (peer.pc.remoteDescription) await peer.pc.addIceCandidate(data.candidate);
      else peer.pendingIce.push(data.candidate); // can outrun the offer or answer
    }
  }

  async #flushIce(peer) {
    for (const c of peer.pendingIce.splice(0)) await peer.pc.addIceCandidate(c);
  }

  #onPeerMessage(id, raw) {
    const player = this.players.get(id);
    const peer = this.peers.get(id);
    if (!player || !peer) return;
    if (!peer.limiter.take()) return; // flooding: drop
    const msg = parsePeerMessage(raw);
    if (msg === null) return;
    if (msg.type === 'chat') {
      if (this.muted.has(id)) return;
      this.h.onChat?.({ from: { id, name: displayName(player.nick, id) }, text: msg.text, mine: false });
    } else {
      this.h.onPeerMessage?.(id, msg);
    }
  }

  #setLink(id, link) {
    const p = this.players.get(id);
    if (p && p.link !== link) {
      p.link = link;
      this.#emitRoster();
    }
  }

  #emitRoster() {
    const me = {
      id: this.myId,
      nick: this.nick,
      name: displayName(this.nick, this.myId ?? 'xxx'),
      status: this.myStatus,
      link: 'self',
      muted: false,
    };
    const others = [...this.players.values()]
      .map((p) => ({ ...p, name: displayName(p.nick, p.id), muted: this.muted.has(p.id) }))
      .sort((a, b) => a.name.localeCompare(b.name));
    this.h.onRoster?.([me, ...others]);
  }
}
