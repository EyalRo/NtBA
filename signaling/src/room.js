import { MAX_MESSAGES_PER_SOCKET, classifyMessage, log, ttlSeconds } from './logic.js';

// One Durable Object per room code. It holds at most two WebSockets (host and guest) and
// relays offer/answer/ICE messages between them. It stores nothing but a creation marker and
// is deleted by an alarm when the room expires.
export class Room {
  constructor(ctx, env) {
    this.ctx = ctx;
    this.env = env;
  }

  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/init' && request.method === 'POST') {
      return this.init();
    }
    if (url.pathname === '/ws') {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return new Response('expected a WebSocket upgrade', { status: 426 });
      }
      const role = url.searchParams.get('role');
      if (role !== 'host' && role !== 'guest') {
        return new Response('role must be host or guest', { status: 400 });
      }
      if (!(await this.exists())) return new Response('no such room', { status: 404 });
      const pair = new WebSocketPair();
      const [client, server] = Object.values(pair);
      const joined = this.join(role, server);
      if (!joined.ok) return new Response(joined.reason, { status: joined.status });
      return new Response(null, { status: 101, webSocket: client });
    }
    return new Response('not found', { status: 404 });
  }

  async exists() {
    return (await this.ctx.storage.get('created')) !== undefined;
  }

  async init() {
    if (await this.exists()) return new Response('room exists', { status: 409 });
    const ttl = ttlSeconds(this.env.ROOM_TTL_SECONDS);
    await this.ctx.storage.put('created', Date.now());
    await this.ctx.storage.setAlarm(Date.now() + ttl * 1000);
    log('INFO', 'room created', { ttl });
    return new Response(JSON.stringify({ ttl }), { status: 201 });
  }

  /** Attach `ws` as `role`. Exposed separately from `fetch` so it can be unit tested. */
  join(role, ws) {
    const sockets = this.ctx.getWebSockets();
    if (sockets.some((s) => this.roleOf(s) === role)) {
      log('WARN', 'role already taken', { role });
      return { ok: false, status: 409, reason: `${role} already connected` };
    }
    this.ctx.acceptWebSocket(ws, [role]);
    ws.serializeAttachment({ role, messages: 0 });
    this.send(ws, { type: 'joined', role });
    const peer = sockets.find((s) => this.roleOf(s) !== role);
    if (peer) {
      // The host drives the handshake: tell it as soon as a guest is present, whichever
      // of the two connected last.
      this.send(role === 'host' ? ws : peer, { type: 'peer-joined' });
    }
    log('INFO', 'peer joined', { role });
    return { ok: true };
  }

  roleOf(ws) {
    return ws.deserializeAttachment()?.role;
  }

  send(ws, obj) {
    try {
      ws.send(JSON.stringify(obj));
    } catch {
      // The other end is already gone; its close handler cleans up.
    }
  }

  async webSocketMessage(ws, data) {
    const att = ws.deserializeAttachment() ?? { messages: 0 };
    att.messages += 1;
    ws.serializeAttachment(att);
    if (att.messages > MAX_MESSAGES_PER_SOCKET) {
      log('WARN', 'too many messages', { role: att.role });
      ws.close(1008, 'too many messages');
      return;
    }
    const result = classifyMessage(data);
    if (result.kind === 'error') {
      log('WARN', 'rejected message', { role: att.role, reason: result.reason });
      this.send(ws, { type: 'error', reason: result.reason });
      return;
    }
    if (result.kind === 'control') {
      // Both peers are connected and negotiated; the room has done its job.
      log('INFO', 'room done', {});
      await this.close(1000, 'done');
      return;
    }
    const peers = this.ctx.getWebSockets().filter((s) => s !== ws);
    if (peers.length === 0) {
      this.send(ws, { type: 'error', reason: 'no-peer' });
      return;
    }
    for (const peer of peers) peer.send(result.text);
  }

  async webSocketClose(ws) {
    this.peerLeft(ws);
  }

  async webSocketError(ws) {
    this.peerLeft(ws);
  }

  peerLeft(ws) {
    log('INFO', 'peer left', { role: this.roleOf(ws) });
    for (const peer of this.ctx.getWebSockets()) {
      if (peer !== ws) this.send(peer, { type: 'peer-left' });
    }
  }

  async alarm() {
    log('INFO', 'room expired', {});
    await this.close(1000, 'room expired');
  }

  async close(code, reason) {
    for (const ws of this.ctx.getWebSockets()) {
      try {
        ws.close(code, reason);
      } catch {
        // Already closed.
      }
    }
    await this.ctx.storage.deleteAll();
  }
}
