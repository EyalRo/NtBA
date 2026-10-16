import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import worker from '../src/index.js';

// A fake ROOMS namespace backed by an in-memory set of "initialised" codes.
function makeEnv(extra = {}) {
  const created = new Set();
  const forwarded = [];
  return {
    forwarded,
    created,
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
          if (url.pathname === '/exists') return new Response(null, { status: created.has(id) ? 200 : 404 });
          forwarded.push({ id, path: url.pathname + url.search });
          return new Response('proxied', { status: 200 });
        },
      }),
    },
    LOBBY: {
      idFromName: (name) => name,
      get: (id) => ({
        fetch: async (req) => {
          forwarded.push({ id, path: new URL(req.url).pathname });
          return new Response('lobby-proxied', { status: 200 });
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
    const env = makeEnv({ TURN_KEY_ID: 'kid', TURN_KEY_API_TOKEN: 'tok' });
    env.created.add('ABC234');
    const res = await worker.fetch(req('/ice?room=ABC234'), env);
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
    const env = makeEnv({ TURN_KEY_ID: 'kid', TURN_KEY_API_TOKEN: 'tok' });
    env.created.add('ABC234');
    const res = await worker.fetch(req('/ice?room=ABC234'), env);
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

test('websocket requests without an upgrade or with a bad role never reach a Durable Object', async () => {
  const env = makeEnv();
  const plain = await worker.fetch(req('/rooms/ABC234/ws?role=host'), env);
  assert.equal(plain.status, 426);
  const badRole = await worker.fetch(req('/rooms/ABC234/ws?role=admin', { headers: { Upgrade: 'websocket' } }), env);
  assert.equal(badRole.status, 400);
  assert.equal(env.forwarded.length, 0);
});

test('room creation uses its own stricter limiter, keyed per route and IP', async () => {
  const seen = [];
  const limiter = (name) => ({ limit: async ({ key }) => (seen.push([name, key]), { success: true }) });
  const env = makeEnv({ LIMITER: limiter('general'), ROOM_LIMITER: limiter('rooms') });
  const headers = { 'CF-Connecting-IP': '203.0.113.9' };
  await worker.fetch(req('/rooms', { method: 'POST', headers }), env);
  await worker.fetch(req('/ice', { headers }), env);
  await worker.fetch(req('/rooms/ABC234/ws?role=host', { headers: { ...headers, Upgrade: 'websocket' } }), env);
  assert.deepEqual(seen, [
    ['rooms', 'rooms:203.0.113.9'],
    ['general', 'ice:203.0.113.9'],
    ['general', 'ws:203.0.113.9'],
  ]);
});

test('a limited websocket join is refused before the Durable Object is touched', async () => {
  const env = makeEnv({ LIMITER: { limit: async () => ({ success: false }) } });
  const res = await worker.fetch(req('/rooms/ABC234/ws?role=host', { headers: { Upgrade: 'websocket' } }), env);
  assert.equal(res.status, 429);
  assert.equal(env.forwarded.length, 0);
});

test('TURN credentials are only issued for a live room, never to an anonymous caller', async () => {
  const realFetch = globalThis.fetch;
  let turnCalls = 0;
  globalThis.fetch = async () => {
    turnCalls++;
    return new Response(
      JSON.stringify({ iceServers: [{ urls: ['turn:turn.cloudflare.com:3478?transport=udp'], username: 'u', credential: 'c' }] }),
      { status: 201 },
    );
  };
  try {
    const env = makeEnv({ TURN_KEY_ID: 'kid', TURN_KEY_API_TOKEN: 'tok' });
    const stunOnly = (body) => assert.deepEqual(body.iceServers, [{ urls: ['stun:stun.cloudflare.com:3478'] }]);

    stunOnly(await (await worker.fetch(req('/ice'), env)).json()); // no room
    stunOnly(await (await worker.fetch(req('/ice?room=ZZZZZZ'), env)).json()); // no such room
    stunOnly(await (await worker.fetch(req('/ice?room=not-a-code'), env)).json()); // malformed
    assert.equal(turnCalls, 0, 'the paid TURN API must not be called');

    env.created.add('ABC234');
    const body = await (await worker.fetch(req('/ice?room=abc-234'), env)).json();
    assert.equal(turnCalls, 1);
    assert.equal(body.iceServers[0].username, 'u');
  } finally {
    globalThis.fetch = realFetch;
  }
});

test('responses are not sniffable and logged origins are bounded', async () => {
  const lines = [];
  console.log = (l) => lines.push(l);
  const long = 'https://' + 'a'.repeat(5000) + '.example';
  const res = await worker.fetch(req('/rooms', { method: 'POST', headers: { Origin: long } }), makeEnv());
  assert.equal(res.status, 403);
  assert.equal(res.headers.get('X-Content-Type-Options'), 'nosniff');
  assert.ok(lines.every((l) => l.length < 400), 'log line was not truncated');
  const ok = await worker.fetch(req('/health'), makeEnv());
  assert.equal(ok.headers.get('X-Content-Type-Options'), 'nosniff');
});

test('GET /lobby/ws without an upgrade is refused', async () => {
  const res = await worker.fetch(req('/lobby/ws', { headers: { Origin: 'https://eyalro.github.io' } }), makeEnv());
  assert.equal(res.status, 426);
});

test('GET /lobby/ws with an upgrade goes to the single main Lobby object', async () => {
  const env = makeEnv();
  const res = await worker.fetch(
    req('/lobby/ws', { headers: { Origin: 'https://eyalro.github.io', Upgrade: 'websocket' } }),
    env,
  );
  assert.equal(res.status, 200);
  assert.deepEqual(env.forwarded, [{ id: 'main', path: '/lobby/ws' }]);
});

test('the lobby route refuses other origins', async () => {
  const res = await worker.fetch(
    req('/lobby/ws', { headers: { Origin: 'https://evil.example', Upgrade: 'websocket' } }),
    makeEnv(),
  );
  assert.equal(res.status, 403);
});

test('the lobby route is rate limited', async () => {
  const env = makeEnv({ LIMITER: { limit: async () => ({ success: false }) } });
  const res = await worker.fetch(
    req('/lobby/ws', { headers: { Origin: 'https://eyalro.github.io', Upgrade: 'websocket' } }),
    env,
  );
  assert.equal(res.status, 429);
  assert.deepEqual(env.forwarded, []);
});
