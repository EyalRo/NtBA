import { MAX_MESSAGES_PER_MINUTE, MAX_PLAYERS, parseClientMessage } from './lobby-logic.js';
import { log } from './logic.js';

// One global Durable Object: who is online, and a relay for the WebRTC handshakes between them.
// It keeps nothing but the roster, held in each socket's attachment so it survives hibernation,
// and it never sees chat: players talk to each other directly.
export class Lobby {
  constructor(ctx) {
    this.ctx = ctx;
  }

  async fetch(request) {
    if (request.headers.get('Upgrade') !== 'websocket') {
      return new Response('expected a WebSocket upgrade', { status: 426 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ joined: false, windowStart: Date.now(), count: 0 });
    return new Response(null, { status: 101, webSocket: client });
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
