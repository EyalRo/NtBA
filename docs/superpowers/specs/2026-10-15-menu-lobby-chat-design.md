# Main menu, online lobby and peer-to-peer chat: design

Status: draft for review. Date: 2026-10-15.

## Goal

Before a game starts the player chooses **Local** or **Online**. Online opens a **lobby** that lists
everyone online, has a chat that works in Hebrew and English, and lets a player challenge another
player into a match. A pixel-art **"Development Version"** badge sits in the top-right corner of the
game screen at the game's own resolution and style.

## Constraints agreed

* Cloudflare is used only to *detect* who is online (presence) and to relay WebRTC handshakes, as it
  does today for matches. Chat, challenges and the game itself travel peer to peer. The Worker never
  sees a chat message.
* No accounts. A player picks a nickname; the visible name is `nick#<3 chars of peer id>`.
* The lobby is a social space that also matches players. Room codes remain as a fallback.
* Delivery is four small PRs, each shippable alone: menu and badge, presence, mesh and chat,
  challenges. This builds on PR #50 (host connect timer).

## Non-goals

Accounts, server-enforced names, chat history, moderation tooling beyond client-side mute, spectators,
rollback, TURN. Lobbies larger than 16 players.

## 1. Menu and badge (PR 1, client only)

* The page opens on a title state: the canvas keeps its idle demo and shows the badge; below it a
  pixel-styled menu offers **Local** and **Online**. Local starts the game as today. Online opens the
  lobby panel.
* The menu and lobby are HTML styled to match the page. The in-canvas font is 3x5 and ASCII-only, so
  Hebrew needs the browser's text rendering.
* Badge: drawn into the 320x200 framebuffer at the top-right with the core's `Framebuffer::text` and
  palette (`crates/ntba-core/src/render.rs`), exposed through the wasm glue so it appears in every
  mode. It is drawn outside the simulation, so game state and hashes are unchanged.
* Tests: a render test that the badge pixels are in the top-right region and the rest of the frame is
  unchanged; `scripts/check-determinism.mjs` and `scripts/test-net-wasm.mjs` still pass.

## 2. Presence (PR 2, Worker)

A single global `Lobby` Durable Object (`idFromName('main')`) behind `GET /lobby/ws`, using WebSocket
hibernation. It stores a roster of `{id, nick, status}` in memory and no message content.

Messages (JSON over WebSocket):

| Direction | Message | Notes |
| --- | --- | --- |
| client -> server | `hello {id, nick}` | `id`: 12 random chars per tab. `nick`: 1-20 chars, trimmed, no control characters. |
| server -> client | `welcome {roster}` | Sent after `hello`. |
| server -> client | `joined {peer}`, `left {id}`, `status {id, status}` | `status` is `lobby` or `in-match`. |
| client -> server | `status {status}` | Updates this player's status. |
| both | `signal {to, data}` / `signal {from, data}` | Server checks `to` is in the roster, rejects payloads over 8 KB, adds `from`, forwards. |
| server -> client | `full` | Sent, then the socket closes, when 16 players are connected. |

Same origin allow-list and per-IP rate limit as the other routes. Logs are structured and carry no
nicknames or payloads. A player is removed from the roster when their socket closes.

Tests (in the style of `signaling/test`): roster add/remove, the cap, `signal` routing and rejection,
size limits, malformed input, duplicate ids.

## 3. Peer mesh and chat (PR 3, client)

* `web/lobby.js` opens the presence socket and keeps one `RTCPeerConnection` with one reliable ordered
  data channel per other player. For each pair, the peer with the lexicographically lower id creates
  the offer, which avoids glare. A failed pair is retried once, then shown as unreachable for chat
  while still listed.
* Peer messages (JSON): `chat {text}` (up to 500 chars), and in PR 4 `challenge {room}`, `accept`,
  `decline`, `cancel`. Every incoming message is validated for type, length and shape.
* `web/chat.js` renders messages with `textContent` only (no HTML injection) and `dir="auto"` per
  message, so Hebrew and English each lay out correctly.
* Limits: 2 outgoing messages per second; incoming above 5 per second from one peer are dropped; a
  per-peer **Mute** button. No history: chat is ephemeral.
* The lobby shows a notice that other players can see your IP address, which is inherent to WebRTC.
* Nickname is remembered in `localStorage` (wrapped in try/catch; the page works without it).
* Tests (node, pure logic): message validation, nickname rules, offer-direction rule, rate limiter.

## 4. Challenges (PR 4, client)

1. The challenger POSTs `/rooms` (existing) and starts hosting. The host's connect timer starts when a
   guest joins (PR #50).
2. They send `challenge {room}` to the target over the data channel.
3. The target sees Accept and Decline, expiring after 30 s.
4. On accept the target joins that room as guest and both run the existing `PeerLink` handshake.
5. The challenge cancels on decline, timeout, or either side leaving.

Players in a match show as `in-match` and cannot be challenged. Leaving a match returns to the lobby.

## Failure handling

* Worker unreachable: the lobby shows "Can't reach the lobby" with a Retry; Local still works.
* Presence socket drops: reconnect with backoff and rebuild the roster.
* Peer unreachable (strict firewall or NAT): that player shows "can't reach for chat or challenges".
  Whether TURN is needed stays a separate decision.

## Risks

* Mesh on restrictive networks (for example a firewalled VPN): roster visible, peers unreachable.
* A global Durable Object with open sockets adds usage; hibernation and the cap of 16 limit it.
* No enforcement against impersonation or abuse beyond the suffix, rate limits and mute.

## Open follow-up (separate from this work)

A guest who joins a room whose host has gone waits 25 s and then sees "Couldn't connect directly".
It should say the host is not there. Small fix in `web/net.js` and `signaling/src/room.js`.
