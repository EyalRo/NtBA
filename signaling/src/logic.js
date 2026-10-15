// Pure helpers shared by the Worker and the Room Durable Object (no runtime APIs, so they
// can be unit tested in plain Node).

export const CODE_ALPHABET = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789'; // no 0/O/1/I/L
export const CODE_LENGTH = 6;
export const MAX_MESSAGE_BYTES = 8 * 1024;
export const MAX_MESSAGES_PER_SOCKET = 100;
export const DEFAULT_TTL_SECONDS = 600;
export const STUN_URLS = ['stun:stun.cloudflare.com:3478'];

// Message types a peer may send; everything is relayed verbatim to the other peer.
export const RELAY_TYPES = new Set(['offer', 'answer', 'ice']);
// `done` is consumed by the room: both peers are connected, so it can close.
export const CONTROL_TYPES = new Set(['done']);

const CODE_RE = new RegExp(`^[${CODE_ALPHABET}]{${CODE_LENGTH}}$`);

/** A random room code from `getRandomValues` (injectable for tests), without modulo bias. */
export function generateCode(getRandomValues = (a) => crypto.getRandomValues(a)) {
  const limit = 256 - (256 % CODE_ALPHABET.length);
  let code = '';
  while (code.length < CODE_LENGTH) {
    const bytes = getRandomValues(new Uint8Array(CODE_LENGTH * 2));
    for (const b of bytes) {
      if (b < limit && code.length < CODE_LENGTH) code += CODE_ALPHABET[b % CODE_ALPHABET.length];
    }
  }
  return code;
}

/** Normalise user input (case, spaces, dashes) and check it is a well-formed code. */
export function normaliseCode(input) {
  const code = String(input ?? '').toUpperCase().replace(/[\s-]/g, '');
  return CODE_RE.test(code) ? code : null;
}

/** Match `/rooms/<code>/ws`; returns the normalised code or null. */
export function parseRoomWsPath(pathname) {
  const m = /^\/rooms\/([^/]+)\/ws$/.exec(pathname);
  if (!m) return null;
  try {
    return normaliseCode(decodeURIComponent(m[1]));
  } catch {
    return null; // malformed percent-encoding
  }
}

/** Browsers always send Origin; a missing Origin (curl, native clients) is allowed. */
export function originAllowed(origin, allowedList) {
  if (!origin) return true;
  const allowed = String(allowedList ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  return allowed.includes(origin);
}

/**
 * Validate one message from a peer. Returns `{ kind: 'relay', text }`, `{ kind: 'control',
 * type }` or `{ kind: 'error', reason }`. Signalling payloads (SDP, candidates) are opaque
 * to the server and are never logged.
 */
export function classifyMessage(data) {
  if (typeof data !== 'string') return { kind: 'error', reason: 'binary-not-allowed' };
  if (new TextEncoder().encode(data).length > MAX_MESSAGE_BYTES) {
    return { kind: 'error', reason: 'message-too-large' };
  }
  let msg;
  try {
    msg = JSON.parse(data);
  } catch {
    return { kind: 'error', reason: 'invalid-json' };
  }
  if (msg === null || typeof msg !== 'object' || Array.isArray(msg)) {
    return { kind: 'error', reason: 'invalid-message' };
  }
  if (RELAY_TYPES.has(msg.type)) return { kind: 'relay', text: data };
  if (CONTROL_TYPES.has(msg.type)) return { kind: 'control', type: msg.type };
  return { kind: 'error', reason: 'unknown-type' };
}

export function ttlSeconds(value) {
  const n = Number.parseInt(value, 10);
  return Number.isFinite(n) && n >= 30 && n <= 3600 ? n : DEFAULT_TTL_SECONDS;
}

/** Structured log line: `{level, msg, ...fields}`; never pass IPs or signalling payloads. */
export function log(level, msg, fields = {}) {
  console.log(JSON.stringify({ level, msg, ...fields }));
}
