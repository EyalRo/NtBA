// Browser side of online play: room setup through the signalling service, the WebRTC
// connection with its two data channels, and nothing else. The lock-step game logic lives in
// the wasm module; this file only moves bytes.
//
// Channels (both pre-negotiated, so no ondatachannel dance):
//   id 0 "reliable"  ordered, retransmitted - handshake, hashes, bye
//   id 1 "fast"      unordered, no retransmits - input windows, pings (a lost packet is
//                    repaired by the next one, so retransmission would only add delay)

const CONNECT_TIMEOUT_MS = 25_000;

export class ConnectError extends Error {}

export class PeerLink {
  /**
   * @param {string} signalingUrl base URL of the signalling service
   * @param {{onCode?: (code: string) => void, onStatus?: (text: string) => void,
   *          onOpen?: () => void, onMessage?: (data: Uint8Array) => void,
   *          onClose?: (reason: string) => void}} handlers
   */
  constructor(signalingUrl, handlers) {
    this.base = signalingUrl.replace(/\/+$/, '');
    this.h = handlers;
    this.pc = null;
    this.ws = null;
    this.reliable = null;
    this.fast = null;
    this.role = null;
    this.open = false;
    this.closed = false;
    this.pendingIce = [];
    this.timer = null;
  }

  async host() {
    const res = await fetch(`${this.base}/rooms`, { method: 'POST' });
    if (res.status === 429) throw new ConnectError('Too many tries - wait a minute and try again.');
    if (!res.ok) throw new ConnectError('Could not create a room (signalling service unavailable).');
    const { code } = await res.json();
    this.h.onCode?.(code);
    await this.#connect('host', code);
    this.h.onStatus?.(`Room ${code} - waiting for the other player…`);
  }

  async join(code) {
    await this.#connect('guest', code);
    this.h.onStatus?.('Joining…');
  }

  async #connect(role, code) {
    this.role = role;
    let iceServers = [{ urls: 'stun:stun.cloudflare.com:3478' }];
    try {
      // Naming the room lets the service hand out relay (TURN) credentials only to real matches.
      const res = await fetch(`${this.base}/ice?room=${encodeURIComponent(code)}`);
      if (res.ok) iceServers = (await res.json()).iceServers ?? iceServers;
    } catch (err) {
      console.warn('[ntba-net] ICE server lookup failed, using public STUN only', err);
    }

    const pc = (this.pc = new RTCPeerConnection({ iceServers }));
    this.reliable = pc.createDataChannel('reliable', { negotiated: true, id: 0, ordered: true });
    this.fast = pc.createDataChannel('fast', { negotiated: true, id: 1, ordered: false, maxRetransmits: 0 });
    for (const dc of [this.reliable, this.fast]) {
      dc.binaryType = 'arraybuffer';
      dc.onopen = () => this.#maybeOpen();
      dc.onclose = () => this.#lost('The connection to the other player closed.');
      dc.onmessage = (e) => this.h.onMessage?.(new Uint8Array(e.data));
    }
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'failed') this.#fail(NO_DIRECT_PATH);
    };
    pc.onicecandidate = (e) => {
      if (e.candidate) this.#signal({ type: 'ice', candidate: e.candidate.toJSON() });
    };

    const ws = (this.ws = new WebSocket(`${this.base.replace(/^http/, 'ws')}/rooms/${code}/ws?role=${role}`));
    ws.onmessage = (e) => this.#onSignal(JSON.parse(e.data)).catch((err) => this.#fail(`Connection setup failed: ${err.message}`));
    // Failing to open at all is reported by the rejection below; these cover later loss.
    await new Promise((resolve, reject) => {
      ws.addEventListener('open', resolve, { once: true });
      ws.addEventListener('error', () => reject(new ConnectError(NO_SIGNALING)), { once: true });
    });
    ws.onerror = () => {
      if (!this.open) this.#fail(NO_SIGNALING);
    };
    ws.onclose = () => {
      if (!this.open && !this.closed) this.#fail('The signalling connection closed before the game connected.');
    };
    this.timer = setTimeout(() => this.#fail(NO_DIRECT_PATH), CONNECT_TIMEOUT_MS);
  }

  #signal(msg) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(msg));
  }

  async #onSignal(msg) {
    const pc = this.pc;
    switch (msg.type) {
      case 'joined':
        break;
      case 'peer-joined': // host only: a guest is here, start the handshake
        this.h.onStatus?.('Player found - connecting…');
        await pc.setLocalDescription(await pc.createOffer());
        this.#signal({ type: 'offer', sdp: pc.localDescription.sdp });
        break;
      case 'offer': // guest only
        await pc.setRemoteDescription({ type: 'offer', sdp: msg.sdp });
        await pc.setLocalDescription(await pc.createAnswer());
        this.#signal({ type: 'answer', sdp: pc.localDescription.sdp });
        await this.#flushIce();
        break;
      case 'answer': // host only
        await pc.setRemoteDescription({ type: 'answer', sdp: msg.sdp });
        await this.#flushIce();
        break;
      case 'ice':
        if (pc.remoteDescription) await pc.addIceCandidate(msg.candidate);
        else this.pendingIce.push(msg.candidate); // can outrun the offer/answer
        break;
      case 'peer-left':
        if (!this.open) this.#fail('The other player left before the game started.');
        break;
      case 'error':
        console.warn('[ntba-net] signalling error', msg.reason);
        break;
    }
  }

  async #flushIce() {
    const queued = this.pendingIce.splice(0);
    for (const c of queued) await this.pc.addIceCandidate(c);
  }

  #maybeOpen() {
    if (this.open || this.reliable.readyState !== 'open' || this.fast.readyState !== 'open') return;
    this.open = true;
    clearTimeout(this.timer);
    // Connected directly: the signalling room has done its job.
    this.#signal({ type: 'done' });
    this.h.onOpen?.();
  }

  get isOpen() {
    return this.open && !this.closed;
  }

  sendReliable(data) {
    if (this.isOpen && this.reliable.readyState === 'open') this.reliable.send(data);
  }

  sendFast(data) {
    if (this.isOpen && this.fast.readyState === 'open') this.fast.send(data);
  }

  #fail(reason) {
    if (this.closed) return;
    this.close();
    this.h.onClose?.(reason);
  }

  #lost(reason) {
    if (this.closed) return;
    this.close();
    this.h.onClose?.(reason);
  }

  close() {
    this.closed = true;
    clearTimeout(this.timer);
    for (const dc of [this.reliable, this.fast]) {
      if (dc) dc.onclose = null;
    }
    try { this.ws?.close(); } catch { /* already closed */ }
    try { this.reliable?.close(); this.fast?.close(); this.pc?.close(); } catch { /* already closed */ }
  }
}

const NO_SIGNALING = 'Could not reach the signalling service (or the room code is wrong or expired).';
const NO_DIRECT_PATH =
  "Couldn't connect directly to the other player (a strict firewall or NAT can block peer-to-peer). Try again, or try a different network.";
