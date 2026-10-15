// End-to-end check against a running `wrangler dev` (real workerd, real Durable Object):
//   npm run dev        # in one terminal
//   npm run e2e        # in another (SIGNALING_URL overrides http://127.0.0.1:8787)
import assert from 'node:assert/strict';

const base = process.env.SIGNALING_URL ?? 'http://127.0.0.1:8787';
const wsBase = base.replace(/^http/, 'ws');

function open(code, role) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${wsBase}/rooms/${code}/ws?role=${role}`);
    ws.inbox = [];
    ws.waiters = [];
    ws.addEventListener('message', (e) => {
      const msg = JSON.parse(e.data);
      const w = ws.waiters.shift();
      if (w) w(msg);
      else ws.inbox.push(msg);
    });
    ws.next = () => new Promise((res) => (ws.inbox.length ? res(ws.inbox.shift()) : ws.waiters.push(res)));
    ws.addEventListener('open', () => resolve(ws));
    ws.addEventListener('error', () => reject(new Error(`websocket ${role} failed`)));
  });
}

const created = await fetch(`${base}/rooms`, { method: 'POST' });
assert.equal(created.status, 201);
const { code } = await created.json();
console.log('room', code);

const host = await open(code, 'host');
assert.deepEqual(await host.next(), { type: 'joined', role: 'host' });
const guest = await open(code.toLowerCase(), 'guest');
assert.deepEqual(await guest.next(), { type: 'joined', role: 'guest' });
assert.deepEqual(await host.next(), { type: 'peer-joined' });

host.send(JSON.stringify({ type: 'offer', sdp: 'OFFER' }));
assert.deepEqual(await guest.next(), { type: 'offer', sdp: 'OFFER' });
guest.send(JSON.stringify({ type: 'answer', sdp: 'ANSWER' }));
assert.deepEqual(await host.next(), { type: 'answer', sdp: 'ANSWER' });
guest.send(JSON.stringify({ type: 'ice', candidate: { c: 1 } }));
assert.deepEqual(await host.next(), { type: 'ice', candidate: { c: 1 } });

// A second host is refused; an unknown room is a 404.
await assert.rejects(open(code, 'host'));
await assert.rejects(open('ZZZZZZ', 'host'));

const ice = await (await fetch(`${base}/ice`)).json();
assert.ok(ice.iceServers[0].urls[0].startsWith('stun:'));

const left = host.next();
guest.close();
assert.deepEqual(await left, { type: 'peer-left' });

host.send(JSON.stringify({ type: 'done' }));
await new Promise((r) => host.addEventListener('close', r));
console.log('signalling e2e ok');
