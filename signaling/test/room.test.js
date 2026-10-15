import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { MAX_MESSAGES_PER_SOCKET } from '../src/logic.js';
import { Room } from '../src/room.js';

// Minimal stand-ins for the Workers runtime objects the Room touches.
class FakeSocket {
  constructor() {
    this.sent = [];
    this.closed = null;
    this.attachment = null;
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
    return this.attachment === null ? null : structuredClone(this.attachment);
  }
}

class FakeCtx {
  constructor() {
    this.sockets = [];
    this.store = new Map();
    this.alarm = null;
  }
  acceptWebSocket(ws) {
    this.sockets.push(ws);
  }
  getWebSockets() {
    return this.sockets.filter((s) => !s.closed);
  }
  storage = {
    get: async (k) => this.store.get(k),
    put: async (k, v) => void this.store.set(k, v),
    setAlarm: async (t) => void (this.alarm = t),
    deleteAll: async () => this.store.clear(),
  };
}

let ctx;
let room;
beforeEach(() => {
  ctx = new FakeCtx();
  room = new Room(ctx, { ROOM_TTL_SECONDS: '120' });
  console.log = () => {}; // silence structured logs in test output
});

const types = (ws) => ws.sent.map((m) => m.type);

test('init creates the room once and schedules expiry', async () => {
  const before = Date.now();
  const res = await room.fetch(new Request('https://room/init', { method: 'POST' }));
  assert.equal(res.status, 201);
  assert.deepEqual(await res.json(), { ttl: 120 });
  assert.ok(ctx.alarm >= before + 120_000);
  const again = await room.fetch(new Request('https://room/init', { method: 'POST' }));
  assert.equal(again.status, 409);
});

test('websocket requests are validated before upgrading', async () => {
  const ws = (qs, headers = { Upgrade: 'websocket' }) => room.fetch(new Request(`https://room/ws${qs}`, { headers }));
  assert.equal((await ws('?role=host', {})).status, 426);
  assert.equal((await ws('?role=admin')).status, 400);
  assert.equal((await ws('?role=host')).status, 404); // room never initialised
});

test('host is told when the guest arrives, whichever joins last', () => {
  const host = new FakeSocket();
  const guest = new FakeSocket();
  assert.deepEqual(room.join('host', host), { ok: true });
  assert.deepEqual(types(host), ['joined']);
  room.join('guest', guest);
  assert.deepEqual(types(guest), ['joined']);
  assert.deepEqual(types(host), ['joined', 'peer-joined']);

  // Reverse order: guest first, then host.
  const c2 = new FakeCtx();
  const r2 = new Room(c2, {});
  const g2 = new FakeSocket();
  const h2 = new FakeSocket();
  r2.join('guest', g2);
  r2.join('host', h2);
  assert.deepEqual(types(h2), ['joined', 'peer-joined']);
  assert.deepEqual(types(g2), ['joined']);
});

test('a role can only be taken once', () => {
  room.join('host', new FakeSocket());
  const second = room.join('host', new FakeSocket());
  assert.equal(second.ok, false);
  assert.equal(second.status, 409);
  assert.equal(ctx.getWebSockets().length, 1);
});

test('offer, answer and ICE are relayed verbatim to the other peer only', async () => {
  const host = new FakeSocket();
  const guest = new FakeSocket();
  room.join('host', host);
  room.join('guest', guest);
  const offer = '{"type":"offer","sdp":"v=0 ..."}';
  await room.webSocketMessage(host, offer);
  assert.deepEqual(guest.sent.at(-1), JSON.parse(offer));
  await room.webSocketMessage(guest, '{"type":"answer","sdp":"a"}');
  await room.webSocketMessage(guest, '{"type":"ice","candidate":{"c":1}}');
  assert.deepEqual(types(host).slice(-2), ['answer', 'ice']);
  assert.ok(!types(host).includes('offer'), 'sender must not get its own message back');
});

test('bad messages are rejected with a reason and not relayed', async () => {
  const host = new FakeSocket();
  const guest = new FakeSocket();
  room.join('host', host);
  room.join('guest', guest);
  const before = guest.sent.length;
  await room.webSocketMessage(host, 'nope');
  await room.webSocketMessage(host, '{"type":"evil"}');
  await room.webSocketMessage(host, new ArrayBuffer(2));
  assert.deepEqual(host.sent.slice(-3).map((m) => m.reason), ['invalid-json', 'unknown-type', 'binary-not-allowed']);
  assert.equal(guest.sent.length, before);
});

test('sending with nobody to receive is reported', async () => {
  const host = new FakeSocket();
  room.join('host', host);
  await room.webSocketMessage(host, '{"type":"offer"}');
  assert.deepEqual(host.sent.at(-1), { type: 'error', reason: 'no-peer' });
});

test('a flooding peer is disconnected', async () => {
  const host = new FakeSocket();
  const guest = new FakeSocket();
  room.join('host', host);
  room.join('guest', guest);
  for (let i = 0; i <= MAX_MESSAGES_PER_SOCKET; i++) {
    await room.webSocketMessage(host, '{"type":"ice"}');
  }
  assert.equal(host.closed.code, 1008);
  assert.equal(guest.closed, null);
});

test('the other peer hears about a departure', async () => {
  const host = new FakeSocket();
  const guest = new FakeSocket();
  room.join('host', host);
  room.join('guest', guest);
  guest.close(1001, 'bye');
  await room.webSocketClose(guest);
  assert.equal(host.sent.at(-1).type, 'peer-left');
});

test('done closes the room and wipes storage', async () => {
  await room.init();
  const host = new FakeSocket();
  const guest = new FakeSocket();
  room.join('host', host);
  room.join('guest', guest);
  await room.webSocketMessage(host, '{"type":"done"}');
  assert.equal(host.closed.reason, 'done');
  assert.equal(guest.closed.reason, 'done');
  assert.equal(ctx.store.size, 0);
});

test('the expiry alarm closes sockets and deletes the room', async () => {
  await room.init();
  const host = new FakeSocket();
  room.join('host', host);
  await room.alarm();
  assert.deepEqual(host.closed, { code: 1000, reason: 'room expired' });
  assert.equal(await room.exists(), false);
});
