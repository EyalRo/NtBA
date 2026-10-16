import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { MAX_MESSAGES_PER_MINUTE, MAX_PLAYERS } from '../src/lobby-logic.js';
import { Lobby } from '../src/lobby.js';

class FakeSocket {
  constructor() {
    this.sent = [];
    this.closed = null;
    this.attachment = { joined: false, windowStart: Date.now(), count: 0, created: Date.now() };
  }
  send(text) {
    if (this.closed) throw new Error('socket closed');
    this.sent.push(JSON.parse(text));
  }
  close(code, reason) {
    this.closed = { code, reason };
  }
  serializeAttachment(v) {
    this.attachment = structuredClone(v);
  }
  deserializeAttachment() {
    return structuredClone(this.attachment);
  }
}

class FakeCtx {
  constructor() {
    this.sockets = [];
  }
  acceptWebSocket(ws) {
    this.sockets.push(ws);
  }
  getWebSockets() {
    return this.sockets.filter((s) => !s.closed);
  }
}

let ctx;
let lobby;
beforeEach(() => {
  ctx = new FakeCtx();
  lobby = new Lobby(ctx);
  console.log = () => {};
});

const idOf = (n) => String(n).padStart(12, 'x');
const hello = (id, nick = 'Dana') => JSON.stringify({ type: 'hello', id, nick });
const types = (ws) => ws.sent.map((m) => m.type);

async function connect(id, nick) {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, hello(id, nick));
  return ws;
}

test('hello gets a welcome with the roster including the newcomer', async () => {
  const a = await connect(idOf(1), 'Ann');
  assert.deepEqual(a.sent[0], {
    type: 'welcome',
    you: { id: idOf(1), nick: 'Ann', status: 'lobby' },
    roster: [{ id: idOf(1), nick: 'Ann', status: 'lobby' }],
  });
});

test('existing players are told about a newcomer and the newcomer sees them', async () => {
  const a = await connect(idOf(1), 'Ann');
  const b = await connect(idOf(2), 'בן');
  assert.deepEqual(a.sent.at(-1), { type: 'joined', peer: { id: idOf(2), nick: 'בן', status: 'lobby' } });
  assert.equal(b.sent[0].roster.length, 2);
});

test('the 17th player gets full and is closed, and does not appear in the roster', async () => {
  const sockets = [];
  for (let i = 1; i <= MAX_PLAYERS; i++) sockets.push(await connect(idOf(i)));
  const extra = await connect(idOf(99));
  assert.deepEqual(extra.sent, [{ type: 'full' }]);
  assert.ok(extra.closed);
  assert.equal(sockets[0].sent.some((m) => m.type === 'joined' && m.peer.id === idOf(99)), false);
});

test('a duplicate id is refused and closed', async () => {
  await connect(idOf(1));
  const dup = await connect(idOf(1), 'Imposter');
  assert.deepEqual(dup.sent, [{ type: 'error', reason: 'duplicate-id' }]);
  assert.ok(dup.closed);
});

test('anything before hello is refused, and hello cannot be sent twice', async () => {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, JSON.stringify({ type: 'status', status: 'lobby' }));
  assert.deepEqual(ws.sent, [{ type: 'error', reason: 'hello-first' }]);
  await lobby.webSocketMessage(ws, hello(idOf(1)));
  await lobby.webSocketMessage(ws, hello(idOf(2)));
  assert.equal(ws.sent.at(-1).reason, 'already-joined');
});

test('a bad nickname is an error, not a join', async () => {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, hello(idOf(1), 'evil\u202ename'));
  assert.deepEqual(ws.sent, [{ type: 'error', reason: 'bad-nick' }]);
});

test('status changes are broadcast to the others', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'status', status: 'in-match' }));
  assert.deepEqual(b.sent.at(-1), { type: 'status', id: idOf(1), status: 'in-match' });
  assert.equal(types(a).includes('status'), false);
});

test('signal is forwarded to the target with from added', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'signal', to: idOf(2), data: { kind: 'offer', sdp: 's' } }));
  assert.deepEqual(b.sent.at(-1), { type: 'signal', from: idOf(1), data: { kind: 'offer', sdp: 's' } });
});

test('signal to someone who is not here is an error to the sender only', async () => {
  const a = await connect(idOf(1));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'signal', to: idOf(7), data: {} }));
  assert.deepEqual(a.sent.at(-1), { type: 'error', reason: 'unknown-peer', to: idOf(7) });
});

test('closing tells the others who left', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  a.close(1000, 'bye');
  await lobby.webSocketClose(a);
  assert.deepEqual(b.sent.at(-1), { type: 'left', id: idOf(1) });
});

test('an unjoined socket closing says nothing', async () => {
  const b = await connect(idOf(2));
  const ghost = new FakeSocket();
  ctx.acceptWebSocket(ghost);
  ghost.close(1000, 'x');
  await lobby.webSocketClose(ghost);
  assert.equal(types(b).includes('left'), false);
});

test('a socket that floods is closed', async () => {
  const a = await connect(idOf(1));
  for (let i = 0; i < MAX_MESSAGES_PER_MINUTE + 5 && !a.closed; i++) {
    await lobby.webSocketMessage(a, JSON.stringify({ type: 'status', status: 'lobby' }));
  }
  assert.deepEqual(a.closed, { code: 1008, reason: 'too many messages' });
});

test('malformed messages get an error and the socket stays open', async () => {
  const a = await connect(idOf(1));
  await lobby.webSocketMessage(a, '{nope');
  assert.deepEqual(a.sent.at(-1), { type: 'error', reason: 'invalid-json' });
  assert.equal(a.closed, null);
});

// ---- admission (the part of fetch() that needs no WebSocketPair) ----

const idle = (ageMs) => {
  const ws = new FakeSocket();
  ws.attachment.created = Date.now() - ageMs;
  ctx.acceptWebSocket(ws);
  return ws;
};

test('admit lets a socket in below twice the player cap', () => {
  for (let i = 0; i < MAX_PLAYERS * 2 - 1; i++) idle(0);
  assert.deepEqual(lobby.admit(), { ok: true });
});

test('admit refuses with 503 at twice the player cap', () => {
  for (let i = 0; i < MAX_PLAYERS * 2; i++) idle(0);
  assert.deepEqual(lobby.admit(), { ok: false, status: 503 });
});

test('admit closes unjoined sockets older than 10 seconds and then has room', () => {
  const stale = [];
  for (let i = 0; i < MAX_PLAYERS * 2; i++) stale.push(idle(11_000));
  assert.deepEqual(lobby.admit(), { ok: true });
  for (const ws of stale) assert.deepEqual(ws.closed, { code: 1008, reason: 'hello timeout' });
});

test('admit survives a stale socket whose close() throws and still admits', () => {
  const stale = [];
  for (let i = 0; i < MAX_PLAYERS * 2; i++) stale.push(idle(11_000));
  stale[0].close = () => {
    throw new Error('already closed');
  };
  assert.deepEqual(lobby.admit(), { ok: true });
  assert.deepEqual(stale[1].closed, { code: 1008, reason: 'hello timeout' });
});

test('admit never closes fresh unjoined sockets or joined players', async () => {
  const fresh = idle(9_000);
  const joined = await connect(idOf(1), 'Ann');
  joined.attachment.created = Date.now() - 60_000; // joined long ago
  assert.deepEqual(lobby.admit(), { ok: true });
  assert.equal(fresh.closed, null);
  assert.equal(joined.closed, null);
});
