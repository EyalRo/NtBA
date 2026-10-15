import { Room } from './room.js';
import {
  STUN_URLS,
  generateCode,
  log,
  normaliseCode,
  originAllowed,
  parseRoomWsPath,
  ttlSeconds,
} from './logic.js';

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
    headers: {
      'Content-Type': 'application/json',
      'Cache-Control': 'no-store',
      'X-Content-Type-Options': 'nosniff',
      ...cors(request, env),
    },
  });
}

/**
 * Per-IP rate limit, counted per route class so a flood of one cannot starve the others.
 * Room creation has its own, stricter binding when configured. A missing binding fails open
 * (the service still works) but is logged, because it means the limits are not in force.
 */
async function limited(request, env, route) {
  const limiter = route === 'rooms' ? (env.ROOM_LIMITER ?? env.LIMITER) : env.LIMITER;
  if (!limiter) {
    log('WARN', 'rate limiter binding missing, requests are not limited', { route });
    return false;
  }
  const ip = request.headers.get('CF-Connecting-IP') ?? 'unknown';
  const { success } = await limiter.limit({ key: `${route}:${ip}` });
  return !success;
}

async function roomExists(env, code) {
  const stub = env.ROOMS.get(env.ROOMS.idFromName(code));
  const res = await stub.fetch('https://room/exists');
  return res.status === 200;
}

/**
 * STUN is public, static information. TURN credentials cost money per GB, so they are only
 * issued to a caller who names a room that currently exists (which itself needs a rate-limited
 * `POST /rooms`), never to an anonymous `GET /ice`.
 */
async function iceServers(env, code) {
  const stun = { urls: STUN_URLS };
  if (!env.TURN_KEY_ID || !env.TURN_KEY_API_TOKEN) return [stun];
  if (!code || !(await roomExists(env, code))) return [stun];
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
      // The origin is attacker-controlled: bound what ends up in the logs.
      log('WARN', 'origin not allowed', { origin: String(origin).slice(0, 100) });
      return new Response('origin not allowed', {
        status: 403,
        headers: { 'X-Content-Type-Options': 'nosniff' },
      });
    }
    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: cors(request, env) });
    }

    if (url.pathname === '/health') return json(request, env, { ok: true });

    if (url.pathname === '/rooms' && request.method === 'POST') {
      if (await limited(request, env, 'rooms')) return json(request, env, { error: 'rate-limited' }, 429);
      return createRoom(request, env);
    }

    if (url.pathname === '/ice' && request.method === 'GET') {
      if (await limited(request, env, 'ice')) return json(request, env, { error: 'rate-limited' }, 429);
      const room = normaliseCode(url.searchParams.get('room'));
      return json(request, env, { iceServers: await iceServers(env, room) });
    }

    const code = parseRoomWsPath(url.pathname);
    if (code && request.method === 'GET') {
      // Reject junk before it can wake (or create) a Durable Object.
      if (request.headers.get('Upgrade') !== 'websocket') {
        return json(request, env, { error: 'websocket-required' }, 426);
      }
      const role = url.searchParams.get('role');
      if (role !== 'host' && role !== 'guest') {
        return json(request, env, { error: 'bad-role' }, 400);
      }
      if (await limited(request, env, 'ws')) return json(request, env, { error: 'rate-limited' }, 429);
      const stub = env.ROOMS.get(env.ROOMS.idFromName(code));
      const target = new URL(request.url);
      target.pathname = '/ws';
      return stub.fetch(new Request(target, request));
    }

    return json(request, env, { error: 'not-found' }, 404);
  },
};
