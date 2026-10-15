import { Room } from './room.js';
import { STUN_URLS, generateCode, log, originAllowed, parseRoomWsPath, ttlSeconds } from './logic.js';

export { Room };

const TURN_TTL_SECONDS = 3600;

function cors(request, env) {
  const origin = request.headers.get('Origin');
  if (!origin || !originAllowed(origin, env.ALLOWED_ORIGINS)) return {};
  return {
    'Access-Control-Allow-Origin': origin,
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
    Vary: 'Origin',
  };
}

function json(request, env, body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...cors(request, env) },
  });
}

async function limited(request, env) {
  if (!env.LIMITER) return false;
  const key = request.headers.get('CF-Connecting-IP') ?? 'unknown';
  const { success } = await env.LIMITER.limit({ key });
  return !success;
}

async function iceServers(env) {
  const stun = { urls: STUN_URLS };
  if (!env.TURN_KEY_ID || !env.TURN_KEY_API_TOKEN) return [stun];
  try {
    const res = await fetch(
      `https://rtc.live.cloudflare.com/v1/turn/keys/${env.TURN_KEY_ID}/credentials/generate-ice-servers`,
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${env.TURN_KEY_API_TOKEN}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ ttl: TURN_TTL_SECONDS }),
      },
    );
    if (!res.ok) throw new Error(`TURN credential API returned ${res.status}`);
    const { iceServers: servers } = await res.json();
    // Port 53 is blocked by browsers and only delays ICE; drop it.
    return servers.map((s) => ({ ...s, urls: [].concat(s.urls).filter((u) => !/:53(\?|$)/.test(u)) }));
  } catch (err) {
    log('ERROR', 'TURN credentials unavailable, serving STUN only', { error: String(err.message ?? err) });
    return [stun];
  }
}

async function createRoom(request, env) {
  for (let attempt = 0; attempt < 5; attempt++) {
    const code = generateCode();
    const stub = env.ROOMS.get(env.ROOMS.idFromName(code));
    const res = await stub.fetch('https://room/init', { method: 'POST' });
    if (res.status === 201) {
      return json(request, env, { code, ttl: ttlSeconds(env.ROOM_TTL_SECONDS) }, 201);
    }
    if (res.status !== 409) break;
  }
  log('ERROR', 'could not allocate a room code');
  return json(request, env, { error: 'unavailable' }, 503);
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const origin = request.headers.get('Origin');
    if (!originAllowed(origin, env.ALLOWED_ORIGINS)) {
      log('WARN', 'origin not allowed', { origin });
      return new Response('origin not allowed', { status: 403 });
    }
    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: cors(request, env) });
    }

    if (url.pathname === '/health') return json(request, env, { ok: true });

    if (url.pathname === '/rooms' && request.method === 'POST') {
      if (await limited(request, env)) return json(request, env, { error: 'rate-limited' }, 429);
      return createRoom(request, env);
    }

    if (url.pathname === '/ice' && request.method === 'GET') {
      if (await limited(request, env)) return json(request, env, { error: 'rate-limited' }, 429);
      return json(request, env, { iceServers: await iceServers(env) });
    }

    const code = parseRoomWsPath(url.pathname);
    if (code && request.method === 'GET') {
      if (await limited(request, env)) return json(request, env, { error: 'rate-limited' }, 429);
      const stub = env.ROOMS.get(env.ROOMS.idFromName(code));
      const target = new URL(request.url);
      target.pathname = '/ws';
      return stub.fetch(new Request(target, request));
    }

    return json(request, env, { error: 'not-found' }, 404);
  },
};
