import {
  IDLE_TIMEOUT_MS,
  MAX_MESSAGES_PER_MINUTE,
  MAX_PLAYERS,
  SWEEP_MS,
  parseClientMessage,
} from './lobby-logic.js';
import { log } from './logic.js';

// One global Durable Object: who is online, and a relay for the WebRTC handshakes between them.
// It keeps nothing but the roster, held in each socket's attachment so it survives hibernation,
// and it never sees chat: players talk to each other directly. Clients ping now and then; an alarm
// closes sockets that have gone quiet (a closed laptop or a lost network never says goodbye),
// so a dead player does not stay in the roster.
const HELLO_DEADLINE_MS = 10_000;

export class Lobby {
  constructor(ctx) {
    this.ctx = ctx;
  }

  async fetch(request) {
    if (request.headers.get('Upgrade') !== 'websocket') {
      return new Response('expected a WebSocket upgrade', { status: 426 });
    }
    const admission = this.admit();
    if (!admission.ok) return new Response('lobby busy', { status: admission.status });
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.ctx.acceptWebSocket(server);
    const now = Date.now();
    server.serializeAttachment({ joined: false, windowStart: now, count: 0, created: now, seen: now });
    await this.armSweep();
    return new Response(null, { status: 101, webSocket: client });
  }

  /** Decide whether one more socket may connect: first close idle sockets that never said hello,
   *  then refuse if twice the player cap are still connected. */
  admit(now = Date.now()) {
    const sockets = this.ctx.getWebSockets();
    let open = sockets.length;
    for (const ws of sockets) {
      const att = ws.deserializeAttachment();
      if (att && att.joined === false && now - att.created > HELLO_DEADLINE_MS) {
        try {
          ws.close(1008, 'hello timeout');
        } catch {
          // Already closing; it no longer counts either way.
          log('DEBUG', 'stale lobby socket was already closed');
        }
        open -= 1;
      }
    }
    return open >= MAX_PLAYERS * 2 ? { ok: false, status: 503 } : { ok: true };
  }

  /** Make sure the idle sweep is scheduled while anyone is connected. */
  async armSweep(now = Date.now()) {
    if ((await this.ctx.storage.getAlarm()) === null) await this.ctx.storage.setAlarm(now + SWEEP_MS);
  }

  /** Close sockets that have not been heard from for IDLE_TIMEOUT_MS and tell the others. */
  async alarm() {
    const now = Date.now();
    const sockets = this.ctx.getWebSockets();
    for (const ws of sockets) {
      const att = ws.deserializeAttachment();
      if (!att || now - (att.seen ?? att.created ?? 0) <= IDLE_TIMEOUT_MS) continue;
      // Mark it gone first so the close handler cannot announce the same player twice.
      ws.serializeAttachment({ ...att, joined: false });
      try {
        ws.close(1001, 'idle');
      } catch {
        // Already closing.
      }
      if (att.joined) {
        log('INFO', 'lobby idle player removed');
        this.broadcast(ws, { type: 'left', id: att.id });
      } else {
        log('INFO', 'lobby idle socket closed');
      }
    }
    if (this.ctx.getWebSockets().length > 0) await this.armSweep(now);
  }

  roster() {
    return this.ctx
      .getWebSockets()
      .map((ws) => ws.deserializeAttachment())
      .filter((a) => a?.joined)
      .map(({ id, nick, status }) => ({ id, nick, status }));
  }

  send(ws, obj) {
    try {
      ws.send(JSON.stringify(obj));
    } catch {
      // Already gone; its close handler cleans up.
    }
  }

  broadcast(except, obj) {
    for (const ws of this.ctx.getWebSockets()) {
      if (ws !== except && ws.deserializeAttachment()?.joined) this.send(ws, obj);
    }
  }

  async webSocketMessage(ws, data) {
    const att = ws.deserializeAttachment();
    const now = Date.now();
    att.seen = now;
    if (now - att.windowStart > 60_000) {
      att.windowStart = now;
      att.count = 0;
    }
    att.count += 1;
    ws.serializeAttachment(att);
    if (att.count > MAX_MESSAGES_PER_MINUTE) {
      log('WARN', 'lobby socket flooding');
      ws.close(1008, 'too many messages');
      return;
    }

    const result = parseClientMessage(data);
    if (result.kind === 'error') {
      log('WARN', 'rejected lobby message', { reason: result.reason });
      this.send(ws, { type: 'error', reason: result.reason });
      return;
    }

    if (result.kind === 'ping') return; // the attachment above already records that it is alive

    if (result.kind === 'hello') {
      if (att.joined) {
        this.send(ws, { type: 'error', reason: 'already-joined' });
        return;
      }
      const roster = this.roster();
      if (roster.some((p) => p.id === result.id)) {
        this.send(ws, { type: 'error', reason: 'duplicate-id' });
        ws.close(1008, 'duplicate id');
        return;
      }
      if (roster.length >= MAX_PLAYERS) {
        log('INFO', 'lobby full');
        this.send(ws, { type: 'full' });
        ws.close(1013, 'lobby full');
        return;
      }
      const me = { id: result.id, nick: result.nick, status: 'lobby' };
      ws.serializeAttachment({ ...att, joined: true, ...me });
      this.send(ws, { type: 'welcome', you: me, roster: [...roster, me] });
      this.broadcast(ws, { type: 'joined', peer: me });
      log('INFO', 'lobby joined', { players: roster.length + 1 });
      return;
    }

    if (!att.joined) {
      this.send(ws, { type: 'error', reason: 'hello-first' });
      return;
    }

    if (result.kind === 'status') {
      ws.serializeAttachment({ ...att, status: result.status });
      this.broadcast(ws, { type: 'status', id: att.id, status: result.status });
      return;
    }

    // signal: forward to the named player, naming the sender.
    const target = this.ctx
      .getWebSockets()
      .find((s) => s.deserializeAttachment()?.joined && s.deserializeAttachment().id === result.to);
    if (!target) {
      this.send(ws, { type: 'error', reason: 'unknown-peer', to: result.to });
      return;
    }
    this.send(target, { type: 'signal', from: att.id, data: result.data });
  }

  async webSocketClose(ws) {
    this.left(ws);
  }

  async webSocketError(ws) {
    this.left(ws);
  }

  left(ws) {
    const att = ws.deserializeAttachment();
    if (!att?.joined) return;
    log('INFO', 'lobby left');
    this.broadcast(ws, { type: 'left', id: att.id });
  }
}
