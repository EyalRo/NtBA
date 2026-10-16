// Pure helpers for the lobby: names, chat and peer-message validation, the offer rule, a rate
// limiter and id generation. No DOM and no network, so Node can test all of it.

export const MAX_CHAT_CHARS = 500;
export const MAX_NICK_CHARS = 20;
const MAX_PEER_MESSAGE_BYTES = 2048;
const ROOM_RE = /^[A-HJKMNP-Z2-9]{6}$/;
const ID_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';

// Control characters, plus the bidi override and isolate characters that can disguise text.
// Hebrew and English letters are untouched.
const BAD_CHARS = /[\u0000-\u001f\u007f-\u009f\u200E\u200F\u202A-\u202E\u2066-\u2069]/;
const BAD_CHARS_G = new RegExp(BAD_CHARS.source, 'g');

/** Same rules as signaling/src/lobby-logic.js `cleanNick`; a test keeps the two in step. */
export function cleanNick(input) {
  // Checked on the raw input so that a newline or tab is rejected rather than collapsed to a space.
  if (typeof input !== 'string' || BAD_CHARS.test(input)) return null;
  const nick = input.trim().replace(/\s+/g, ' ');
  const length = [...nick].length;
  return length >= 1 && length <= MAX_NICK_CHARS ? nick : null;
}

/** Chat text with unsafe characters removed and trimmed, or null if empty or too long. */
export function cleanChat(text) {
  if (typeof text !== 'string') return null;
  const clean = text.replace(BAD_CHARS_G, '').trim();
  const length = [...clean].length;
  return length >= 1 && length <= MAX_CHAT_CHARS ? clean : null;
}

export const displayName = (nick, id) => `${nick}#${id.slice(0, 3).toUpperCase()}`;

/** For each pair the lower id creates the WebRTC offer, so both sides never offer at once. */
export const shouldOffer = (myId, theirId) => myId < theirId;

/** Validate a message from another player. Returns a clean message or null. */
export function parsePeerMessage(text) {
  if (typeof text !== 'string' || new TextEncoder().encode(text).length > MAX_PEER_MESSAGE_BYTES) return null;
  let msg;
  try {
    msg = JSON.parse(text);
  } catch {
    return null;
  }
  if (msg === null || typeof msg !== 'object' || Array.isArray(msg)) return null;
  switch (msg.type) {
    case 'chat': {
      const clean = cleanChat(msg.text);
      return clean === null ? null : { type: 'chat', text: clean };
    }
    case 'challenge':
      return typeof msg.room === 'string' && ROOM_RE.test(msg.room) ? { type: 'challenge', room: msg.room } : null;
    case 'accept':
    case 'decline':
    case 'cancel':
      return { type: msg.type };
    default:
      return null;
  }
}

/** Token bucket: `take()` is true if an action is allowed now. */
export function createRateLimiter({ perSecond, burst = perSecond, now = () => performance.now() }) {
  let tokens = burst;
  let last = now();
  return {
    take() {
      const t = now();
      tokens = Math.min(burst, tokens + ((t - last) / 1000) * perSecond);
      last = t;
      if (tokens >= 1) {
        tokens -= 1;
        return true;
      }
      return false;
    },
  };
}

/** 12 random letters and digits, without modulo bias. */
export function randomId(getRandomValues = (a) => crypto.getRandomValues(a)) {
  const limit = 256 - (256 % ID_ALPHABET.length);
  let id = '';
  while (id.length < 12) {
    for (const b of getRandomValues(new Uint8Array(24))) {
      if (b < limit && id.length < 12) id += ID_ALPHABET[b % ID_ALPHABET.length];
    }
  }
  return id;
}
