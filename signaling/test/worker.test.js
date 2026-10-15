import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import worker from '../src/index.js';

// A fake ROOMS namespace backed by an in-memory set of "initialised" codes.
function makeEnv(extra = {}) {
  const created = new Set();
  const forwarded = [];
  return {
    forwarded,
    ALLOWED_ORIGINS: 'https://eyalro.github.io',
    ROOM_TTL_SECONDS: '600',
    ROOMS: {
      idFromName: (name) => name,
      get: (id) => ({
        fetch: async (req) => {
          const url = new URL(typeof req === 'string' ? req : req.url);
          if (url.pathname === '/init') {
            if (created.has(id)) return new Response('exists', { status: 409 });
            created.add(id);
            return new Response('{}', { status: 201 });
          }
          forwarded.push({ id, path: url.pathname + url.search });
          return new Response('proxied', { status: 200 });
        },
      }),
    },
    ...extra,
  };
}

const req = (path, init = {}) => new Request(`https://signal.example${path}`, init);

beforeEach(() => {
  console.log = () => {};
});

test('POST /rooms returns a code and CORS headers for an allowed origin', async () => {
  const res = await worker.fetch(req('/rooms', { method: 'POST', headers: { Origin: 'https://eyalro.github.io' } }), makeEnv());
  assert.equal(res.status, 201);
  const body = await res.json();
  assert.match(body.code, /^[A-Z2-9]{6}$/);
  assert.equal(body.ttl, 600);
  assert.equal(res.headers.get('Access-Control-Allow-Origin'), 'https://eyalro.github.io');
});

test('other origins are refused', async () => {
  const res = await worker.fetch(req('/rooms', { method: 'POST', headers: { Origin: 'https://evil.example' } }), makeEnv());
  assert.equal(res.status, 403);
});

test('preflight succeeds for an allowed origin', async () => {
  const res = await worker.fetch(req('/rooms', { method: 'OPTIONS', headers: { Origin: 'https://eyalro.github.io' } }), makeEnv());
  assert.equal(res.status, 204);
  assert.match(res.headers.get('Access-Control-Allow-Methods'), /POST/);
});

test('rate limiting returns 429', async () => {
  const env = makeEnv({ LIMITER: { limit: async () => ({ success: false }) } });
  const res = await worker.fetch(req('/rooms', { method: 'POST' }), env);
  assert.equal(res.status, 429);
});

test('a code collision is retried', async () => {
  const env = makeEnv();
  let calls = 0;
  const realGet = env.ROOMS.get;
  env.ROOMS.get = (id) => {
    const stub = realGet(id);
    return {
      fetch: async (r) => (calls++ === 0 ? new Response('exists', { status: 409 }) : stub.fetch(r)),
    };
  };
  const res = await worker.fetch(req('/rooms', { method: 'POST' }), env);
  assert.equal(res.status, 201);
  assert.equal(calls, 2);
});

test('websocket upgrades are forwarded to the room object with the normalised code', async () => {
  const env = makeEnv();
  const res = await worker.fetch(req('/rooms/abc-234/ws?role=guest', { headers: { Upgrade: 'websocket' } }), env);
  assert.equal(res.status, 200);
  assert.deepEqual(env.forwarded, [{ id: 'ABC234', path: '/ws?role=guest' }]);
});

test('malformed room codes never reach a Durable Object', async () => {
  const env = makeEnv();
  const res = await worker.fetch(req('/rooms/../ws'), env);
  assert.equal(res.status, 404);
  assert.equal(env.forwarded.length, 0);
});

test('/ice serves STUN only without TURN secrets', async () => {
  const res = await worker.fetch(req('/ice'), makeEnv());
  const { iceServers } = await res.json();
  assert.deepEqual(iceServers, [{ urls: ['stun:stun.cloudflare.com:3478'] }]);
});

test('/ice adds short-lived TURN credentials when configured and drops port 53', async () => {
  const realFetch = globalThis.fetch;
  let seen;
  globalThis.fetch = async (url, init) => {
    seen = { url: String(url), auth: init.headers.Authorization, body: init.body };
    return new Response(
      JSON.stringify({
        iceServers: [
          { urls: ['stun:stun.cloudflare.com:3478', 'stun:stun.cloudflare.com:53'] },
          { urls: ['turn:turn.cloudflare.com:3478?transport=udp', 'turn:turn.cloudflare.com:53?transport=udp'], username: 'u', credential: 'c' },
        ],
      }),
      { status: 201 },
    );
  };
  try {
    const res = await worker.fetch(req('/ice'), makeEnv({ TURN_KEY_ID: 'kid', TURN_KEY_API_TOKEN: 'tok' }));
    const { iceServers } = await res.json();
    assert.match(seen.url, /turn\/keys\/kid\/credentials\/generate-ice-servers$/);
    assert.equal(seen.auth, 'Bearer tok');
    assert.deepEqual(JSON.parse(seen.body), { ttl: 3600 });
    assert.deepEqual(iceServers[0].urls, ['stun:stun.cloudflare.com:3478']);
    assert.deepEqual(iceServers[1].urls, ['turn:turn.cloudflare.com:3478?transport=udp']);
    assert.equal(iceServers[1].username, 'u');
  } finally {
    globalThis.fetch = realFetch;
  }
});

test('/ice falls back to STUN when the TURN API fails', async () => {
  const realFetch = globalThis.fetch;
  globalThis.fetch = async () => new Response('boom', { status: 500 });
  try {
    const res = await worker.fetch(req('/ice'), makeEnv({ TURN_KEY_ID: 'kid', TURN_KEY_API_TOKEN: 'tok' }));
    const { iceServers } = await res.json();
    assert.equal(iceServers.length, 1);
    assert.equal(res.status, 200);
  } finally {
    globalThis.fetch = realFetch;
  }
});

test('unknown routes are 404 and health works', async () => {
  assert.equal((await worker.fetch(req('/nope'), makeEnv())).status, 404);
  assert.deepEqual(await (await worker.fetch(req('/health'), makeEnv())).json(), { ok: true });
});
