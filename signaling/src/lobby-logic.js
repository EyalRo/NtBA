// Pure helpers for the Lobby Durable Object (no runtime APIs, so plain Node can test them).

export const MAX_PLAYERS = 16;
export const MAX_NICK_CHARS = 20;
export const MAX_LOBBY_MESSAGE_BYTES = 8 * 1024;
export const MAX_MESSAGES_PER_MINUTE = 600;
// Clients send a ping this often; a socket silent for IDLE_TIMEOUT_MS is treated as dead. The
// timeout is generous because background tabs throttle timers to about once a minute.
export const HEARTBEAT_MS = 25_000;
export const IDLE_TIMEOUT_MS = 150_000;
export const SWEEP_MS = 30_000;

const STATUSES = new Set(['lobby', 'in-match']);
const ID_RE = /^[A-Za-z0-9]{12}$/;
// Control characters, plus the bidi marks, overrides and isolates that can make one name
// read as another. Hebrew and English letters are fine.
const BAD_NICK_CHARS = /[\u0000-\u001f\u007f-\u009f\u061c\u200e\u200f\u202a-\u202e\u2066-\u2069]/;

/** Trimmed, single-spaced nickname of 1-20 characters, or null if it is not acceptable. */
export function cleanNick(input) {
  // Checked on the raw input so that a newline or tab is rejected rather than collapsed to a space.
  if (typeof input !== 'string' || BAD_NICK_CHARS.test(input)) return null;
  const nick = input.trim().replace(/\s+/g, ' ');
  const length = [...nick].length;
  return length >= 1 && length <= MAX_NICK_CHARS ? nick : null;
}

/**
 * Validate one message from a client. Signalling payloads (SDP, candidates) are opaque to the
 * server and are never logged.
 */
export function parseClientMessage(data) {
  if (typeof data !== 'string') return { kind: 'error', reason: 'binary-not-allowed' };
  if (new TextEncoder().encode(data).length > MAX_LOBBY_MESSAGE_BYTES) {
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
  switch (msg.type) {
    case 'hello': {
      if (typeof msg.id !== 'string' || !ID_RE.test(msg.id)) return { kind: 'error', reason: 'bad-id' };
      const nick = cleanNick(msg.nick);
      if (nick === null) return { kind: 'error', reason: 'bad-nick' };
      return { kind: 'hello', id: msg.id, nick };
    }
    case 'ping':
      return { kind: 'ping' };
    case 'status':
      if (!STATUSES.has(msg.status)) return { kind: 'error', reason: 'bad-status' };
      return { kind: 'status', status: msg.status };
    case 'signal':
      if (typeof msg.to !== 'string' || !ID_RE.test(msg.to)) return { kind: 'error', reason: 'bad-target' };
      if (msg.data === null || typeof msg.data !== 'object' || Array.isArray(msg.data)) {
        return { kind: 'error', reason: 'bad-signal' };
      }
      return { kind: 'signal', to: msg.to, data: msg.data };
    default:
      return { kind: 'error', reason: 'unknown-type' };
  }
}
