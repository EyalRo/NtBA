# Online play (peer-to-peer)

Two people play each other over the internet with **no game server**. The only infrastructure
is a tiny signalling service that tells browsers who is online and helps them find each other;
once connected, the players talk directly (match traffic, chat and challenges never pass through
the service).

Players meet in an **online lobby**: pick a name, see who else is online, chat, and challenge
someone to a match. Room codes (Host / Join with code) remain as a fallback on the same page.

Tracking issue: [#9](https://github.com/EyalRo/NtBA/issues/9). Implementation steps:
[#40](https://github.com/EyalRo/NtBA/issues/40) design (this page),
[#41](https://github.com/EyalRo/NtBA/issues/41) state hash and replay,
[#42](https://github.com/EyalRo/NtBA/issues/42) wire protocol and transport,
[#43](https://github.com/EyalRo/NtBA/issues/43) lock-step session,
[#44](https://github.com/EyalRo/NtBA/issues/44) signalling service,
[#45](https://github.com/EyalRo/NtBA/issues/45) WebRTC and lobby,
[#46](https://github.com/EyalRo/NtBA/issues/46) desync and disconnect handling.

## Decisions at a glance

| Question | Decision |
| --- | --- |
| Sync model | **Deterministic delay-based lock-step**: peers exchange *inputs* (one byte per team per tick), never game state. |
| Transport | **WebRTC data channels**: one reliable and ordered, one unreliable and unordered. |
| Finding each other | Cloudflare Worker with **one Durable Object per room** (6-character room codes) and **one global `Lobby` Durable Object** that only detects who is online and relays WebRTC handshakes. |
| Chat and challenges | Peer to peer over WebRTC data channels in a full mesh between lobby players; the service never sees them. |
| NAT traversal | **STUN** (Cloudflare, free) always; **TURN** is optional and off until the operator adds credentials. |
| Authority | None needed: both peers simulate the same game and check each other with state hashes. |
| Desync | State hash every 60 ticks; a mismatch stops the match with an explanation. |
| Versioning | The `Hello` message carries the protocol version and a **build id** (a hash of the simulation's behaviour); mismatches refuse to play. |
| Disconnects | Distinct outcomes for quit, lost connection, timeout (5 s of silence), desync and version mismatch. No reconnect. |

## Why lock-step and not host-authoritative snapshots

Two designs were considered:

1. **Host-authoritative:** the host runs the game and streams positions and the ball to the
   guest, who interpolates. It needs a snapshot format for every part of the game, interpolation,
   client-side prediction to hide a full round trip of input lag, and more bandwidth. The host can
   also cheat, and the match dies with the host.
2. **Lock-step (chosen):** the core was *already built for this*. The simulation is fixed-step
   (60 Hz), never reads a clock, takes input as one byte per team per tick, uses a seeded RNG and
   sticks to `+ - * /` and `sqrt` on `f32` in `game.rs` so results are bit-identical on every
   machine. So both peers just run the same game and swap input bytes: a few bytes per tick, no
   snapshot or interpolation code, no host advantage, and the same input log doubles as a replay.

The price of lock-step is **input delay** (your own input is applied a few ticks after you
press it, currently 6 ticks = 100 ms, so it can cross the network in time) and the need for
*exact* determinism, which is why every online match checks state hashes. Rollback (apply input
immediately, rewind when the remote input differs) is a possible later upgrade; the wire
protocol would not change.

## Architecture

```mermaid
flowchart LR
  subgraph A[Browser A, host, plays Home]
    CA[ntba-core in wasm] --- SA[ntba-net Session]
  end
  subgraph B[Browser B, guest, plays Away]
    CB[ntba-core in wasm] --- SB[ntba-net Session]
  end
  SA <-- "WebRTC data channels: inputs, hashes" --> SB
  W[(Cloudflare Worker + Room Durable Object)]
  A -. "room code, offer / answer / ICE (setup only)" .-> W
  B -. "room code, offer / answer / ICE (setup only)" .-> W
  L[(Lobby Durable Object)]
  A -. "presence: who is online, handshake relay" .-> L
  B -. "presence: who is online, handshake relay" .-> L
  A <-- "lobby mesh: chat, challenges (peer to peer)" --> B
```

The Room Durable Object pairs two players for one match. The Lobby Durable Object only tracks who
is online and relays the WebRTC handshakes for the lobby mesh, a direct data channel between every
two lobby players that carries chat and challenges.

| Piece | Where | Role |
| --- | --- | --- |
| `ntba-core` | `crates/ntba-core` | The deterministic simulation. Adds `Game::state_hash()` and `replay::InputLog`. |
| `ntba-net` | `crates/ntba-net` | Wire protocol, `Transport` trait, simulated `LocalLink`, lock-step `Session`, level-based logging. No browser or OS dependency, so a native client can reuse it. |
| `ntba-wasm` | `crates/ntba-wasm/src/net.rs` | C-ABI glue: JavaScript hands received bytes in and drains bytes to send. No wasm imports needed. |
| `web/net.js` | `web/` | Room signalling client and the `RTCPeerConnection` with its two match data channels. Moves bytes only. |
| `web/main.js` | `web/` | Page wiring and the same fixed-timestep loop, driving the session instead of the solo game. |
| `web/flow.js` | `web/` | Pure screen state machine: title, local, online, match. |
| `web/lobby.js` | `web/` | Lobby client: presence socket, mesh of peer connections, chat and challenge messages. |
| `web/lobby-protocol.js` | `web/` | Pure helpers: name and chat cleaning, peer-message validation, offer rule, rate limiter, id generation. |
| `web/challenge.js` | `web/` | Pure challenge reducer. |
| `web/chat.js`, `web/lobby-ui.js` | `web/` | Chat log and player-list rendering (text only). |
| `signaling/` | repo root | The Cloudflare Worker with the `Room` and `Lobby` Durable Objects. |

## The lock-step session

* The **host** picks the seed and the input delay and plays Home; the **guest** adopts both from
  the host's `Hello` and plays Away.
* Each frame the session samples the local pad **only when it needs a new input** (so a stall never
  silently increases the delay), appends it to a queue `delay` ticks ahead, and sends **every input
  byte the peer has not acknowledged yet** (oldest first, up to 128) on the unreliable channel. A lost
  packet is therefore repaired by the next one, with no retransmission delay.
* A tick runs only when both pads for it are known. If the peer's byte is missing the session
  **stalls** rather than guessing; the web loop does not accumulate a catch-up burst while stalled.
* After every 60 ticks both peers send a hash of the full game state; a mismatch sends `Bye(Desync)`
  and ends both sides with the tick and both hashes in the log.
* `Ping`/`Pong` every 30 frames measure round-trip time (shown as "ping" in the page).
* The session records every simulated tick, so an online match can be exported as an `InputLog` and
  replayed offline.

### Messages

All messages are `tag: u8` plus fixed little-endian fields. Decoding is strict (unknown tags,
truncation, trailing bytes, bad values and oversized input runs are errors, never panics), because
the bytes come from another machine.

| Message | Channel | Purpose |
| --- | --- | --- |
| `Hello { protocol, build, seed, side, input_delay }` | reliable | First message each way; version check, seed and delay from the host. |
| `Inputs { ack, first_tick, pads[] }` | unreliable | Redundant window of the sender's pad bytes plus an acknowledgement. |
| `Hash { tick, hash }` | reliable | State hash after `tick` ticks. |
| `Ping`/`Pong { id }` | unreliable | Round-trip time and liveness. |
| `Bye { reason }` | reliable | Quit, desync or version mismatch. |

## Connection flow

1. The host calls `POST /rooms` and gets a code such as `K7M2QX` (valid for 10 minutes), opens the
   room WebSocket as `host`, and shows the code. The host waits for a guest for as long as the room
   lives.
2. The guest enters the code and opens the WebSocket as `guest`. The room tells the host
   `peer-joined`.
3. The host creates a WebRTC offer; offer, answer and ICE candidates travel through the room.
4. Both data channels are **pre-negotiated** (ids 0 and 1), so there is no `ondatachannel` step.
   When both open, each side sends `done` and the room closes; the match starts with `Hello`.
5. The 25 s connection timer starts when a guest actually joins (host) or when the guest connects.
   If no direct path forms within it (or the browser reports `failed`), the page says it could not
   connect directly and offers to try again.

A challenge from the lobby uses this same flow: the challenger hosts a room and the accepted target
joins it as guest (see *Challenges* below).

### Data channels

| Channel | Settings | Carries |
| --- | --- | --- |
| `reliable` (id 0) | ordered, retransmitted | `Hello`, `Hash`, `Bye` |
| `fast` (id 1) | unordered, `maxRetransmits: 0` | `Inputs`, `Ping`, `Pong` |

One reliable channel for everything would let a single lost packet stall all later inputs
(head-of-line blocking). Inputs are sent as a repeating window instead.

## The lobby

Choosing **Online** on the title screen opens the lobby.

* **Name.** You pick a name of 1 to 20 characters; letters, digits, Hebrew and emoji are fine, while
  control characters and bidi override characters are rejected. It is remembered in the browser's
  `localStorage` when that is available. Others see you as `name#ABC`: the first three characters of
  a random 12-character peer id that is regenerated on every connection, so same-named players can be
  told apart.
* **Roster.** The lobby lists everyone online with their state (in the lobby or in a match) and
  whether you are connected to them. At most 16 players fit; a 17th sees "lobby full".
* **Notice.** The lobby tells players that others in it can see their IP address, which is how
  peer-to-peer works.

### How the lobby works

Cloudflare only *detects* who is online. One global `Lobby` Durable Object, reached at
`GET /lobby/ws`, keeps the roster in WebSocket attachments (so it survives hibernation) and relays
WebRTC handshakes between players by id. It never sees chat and stores nothing.

| Direction | Message | Meaning |
| --- | --- | --- |
| client to server | `hello { id, nick }` | Join the roster. |
| client to server | `status { status }` | `lobby` or `in-match`. |
| client to server | `signal { to, data }` | Relay a handshake message to player `to`. |
| server to client | `welcome { you, roster }` | Joined; here is everyone. |
| server to client | `joined`, `left`, `status` | Roster changes. |
| server to client | `signal { from, data }` | A relayed handshake message. |
| server to client | `full` | The lobby has 16 players. |
| server to client | `error { reason }` | A refused message. |

Every player then opens a direct WebRTC data channel to every other player (a full mesh). For each
pair the peer with the lower id makes the offer. A pair that fails to connect within 20 s is retried
once, by the offering side, and is then shown as "can't reach for chat or challenges". The presence
socket reconnects with backoff (1, 2, 4, 8, then 15 s) using a fresh id, and a player who is in a
match tells the lobby so again after every reconnect.

If the lobby cannot be reached, the page says "Can't reach the lobby. Retrying…" (the room code
below still works); after a connection that was working drops it says "Lost the lobby connection -
retrying…". While it is retrying, or when the lobby is full, a **Retry** button tries again at once.

### Chat

* Messages go peer to peer over the lobby data channels. Nothing is stored anywhere and there is no
  history.
* Hebrew and English both work. Each message is its own `dir="auto"` block, so Hebrew lines
  right-align and English lines left-align.
* Text is inserted as text only, never HTML, and bidi override and control characters are stripped.
* Messages are up to 500 characters. You can send 2 messages per second. Messages from a peer above
  5 per second (after a burst of 10) are dropped, as are oversized or malformed ones; the peer is not
  disconnected.
* Each player has a **Mute** button.

### Challenges

* A **Challenge** button sits next to each connected player who is in the lobby.
* The challenger creates a room (`POST /rooms`), hosts it, and sends `challenge { room }` over the
  data channel. The target sees Accept and Decline for 30 seconds.
* On accept the target joins the room as guest and both run the normal match handshake.
* Decline, timeout, cancel, or either side leaving cancels the challenge and closes the hosted room.
  Once the target has accepted, the challenger waits at most another 30 seconds for them to connect to
  the room; after that the challenge is cancelled with "They didn't connect - try again." A brief loss
  of the target's lobby connection during that wait does not cancel it. If both players challenge each other at once, both challenges are cancelled with a message.
* While in a match a player shows as "in a match" and cannot be challenged; this holds after a lobby
  reconnect too, and a challenge that arrives while you are busy is declined automatically.
* A muted player's challenges are declined automatically.
* The Accept button is disabled while you are hosting or joining a room.
* The rules live in the pure reducer `web/challenge.js`, which takes an event and returns the new
  state plus effects for the page to carry out.

## The signalling service

Source, security model and operator notes: [Signalling Service](Signaling-Service).

| Route | Purpose |
| --- | --- |
| `POST /rooms` | Create a room: `{ code, ttl }` |
| `GET /rooms/<code>/ws?role=host\|guest` | WebSocket into the room's Durable Object |
| `GET /lobby/ws` | WebSocket into the global `Lobby` Durable Object |
| `GET /ice` | `{ iceServers }`: STUN, plus short-lived TURN credentials when configured |
| `GET /health` | Liveness |

* **Why a Durable Object per room, not a plain stateless Worker.** Two requests for the same room
  can be handled by *different isolates*, which do not share memory, so an in-memory room table
  would randomly say "room not found". A Durable Object is a single addressable instance per room
  and is available on the Workers Free plan (SQLite-backed).
* The room holds at most one `host` and one `guest` socket, relays `offer`, `answer` and `ice`
  verbatim, rejects anything else, caps messages at 8 KiB and 100 per socket, and deletes itself
  after `ROOM_TTL_SECONDS` (default 600) or when a peer sends `done`.
* Other protections: an `ALLOWED_ORIGINS` allow-list, a per-IP Workers rate-limit binding, bias-free
  room codes from an alphabet without look-alike characters.
* **Privacy:** nothing about players is stored; signalling payloads and IP addresses are never logged.
  (As with any WebRTC game, the two players do learn each other's IP address.)
* The service never sees gameplay, chat or challenges.
* The `Lobby` object limits whole messages to 8 KB and each socket to 600 messages per minute,
  refuses new connections with 503 when more than twice the player cap (32) of sockets are connected,
  closes sockets that have not said hello within 10 seconds, refuses duplicate ids, and uses the same origin allow-list and per-IP rate limit as the other
  routes. Its logs carry no nicknames or payloads.

## NAT traversal, STUN and TURN

Direct connections cannot be guaranteed between every pair of networks (symmetric NAT, some
mobile carriers, strict firewalls). The page uses Cloudflare's STUN service
(`stun.cloudflare.com`, free) for every game. For the pairs that still cannot connect, a **TURN
relay** is supported but **off by default**: set `TURN_KEY_ID` and `TURN_KEY_API_TOKEN` as Worker
secrets (never in the repo) and `/ice` starts returning short-lived TURN credentials.

Per Cloudflare's documentation STUN is free and unlimited, and TURN is
$0.05 per GB sent from Cloudflare with a monthly free allowance of 1,000 GB shared with Cloudflare's
other real-time services. An NtBA match sends on the order of 1 to 2 KB per second, so the free
allowance would go a very long way; the default is *no relay* so that nothing
quietly turns into a bandwidth-bearing server. Check Cloudflare's current pricing before enabling it.

## Determinism

* `Game::state_hash()` is a 64-bit FNV-1a over every field that affects the simulation (floats by
  bit pattern), so any divergence shows up.
* `replay::InputLog` records a seed and the pads for each tick and can replay them.
* A **golden hash** (`DETERMINISM_SEED_HASH` in `ntba-core`) pins the result of replaying a fixed
  synthetic 3,600-tick script. A native unit test checks it, and `scripts/check-determinism.mjs`
  runs the *built wasm* in Node and checks it too; both are in CI. Native and wasm currently agree.
* The golden hash is also the **build id** peers compare in `Hello`: if you change simulation
  behaviour on purpose, update the constant and peers on the old version will refuse to match.
* A changed input can legitimately converge back to the same state a few hundred ticks later (a
  player pinned against the boundary, a possession reset), so tests compare hashes at the tick of
  the change, not just at the end.

## Failure handling

| Outcome | What the player sees |
| --- | --- |
| Other player pressed Leave | "The other player left the match." |
| You pressed Leave | "You left the online match." |
| Hash mismatch | "The two games went out of sync, so the match was stopped." |
| Different protocol or build | "…running a different version of the game. Reload both pages and try again." |
| Transport closed or failed | "The connection to the other player was lost." |
| 5 s of silence | "The other player stopped responding." |
| Invalid data from the peer | "The other player sent something unexpected…" |
| No direct path at setup | "Couldn't connect directly… try again, or try a different network." |

While the peer's input is late the match simply pauses (stalls) until it arrives.

## Logging

All layers use the standard levels DEBUG, INFO, WARN and ERROR. The Rust net layer has no I/O of its
own: the host installs a sink (the page forwards to `console.debug/info/warn/error`). The Worker writes
one JSON object per line with a `level` field.

## Verification

| Check | Where |
| --- | --- |
| Replay and state hash behaviour, golden hash | `ntba-core` tests |
| Native and wasm simulate bit-identically | `scripts/check-determinism.mjs` (CI) |
| Protocol round trips, truncation and garbage rejection | `ntba-net` tests |
| Lock-step in sync over latency, jitter and 25% loss; stall behaviour; desync detection; version refusal; quit, sever, timeout | `ntba-net` session tests over the simulated link |
| Two wasm instances over a lossy link end on the same tick with the same hash | `scripts/test-net-wasm.mjs` (CI) |
| Signalling logic, room object, lobby object and Worker routes | `signaling` unit tests (`npm test`, CI), with fake sockets |
| Pure web modules: screen flow, lobby protocol, challenge reducer, lobby client | `node --test web/test/*.test.js` (CI) |
| Real Durable Object, real WebSockets | `signaling` end-to-end script against `wrangler dev` |
| Two real Chromium pages, real WebRTC, real signalling server | manual run (sides, seed, ping, same tick at the end, leave) |

## Running it

```sh
cd signaling && npm install && npm run dev     # signalling on http://127.0.0.1:8787
# point SIGNALING_URL in web/config.js at that URL (the committed value is the deployed service), then in the repo root:
make serve                                    # game on http://localhost:8080
```

Open the page in two windows and choose **Online** in each: pick names and challenge one from the
other, or use **Host an online game** in one and type its code into **Join with code** in the other.
The Online button is hidden while `SIGNALING_URL` is empty or WebRTC is unavailable. The production
setting points at the deployed Worker.

To deploy: `npm run deploy` in `signaling/` (needs a Cloudflare login or `CLOUDFLARE_API_TOKEN` in
your environment), add the game's origin to `ALLOWED_ORIGINS`, and put the Worker URL in
`web/config.js`.

## Known limitations / not done yet

* Rollback, spectators and reconnecting into a match are not implemented.
* TURN relay is supported by the Worker but off, so some firewall and NAT combinations (for example
  some VPNs and strict networks) cannot connect peer to peer.
* Cross-browser (Firefox, Safari) determinism and cross-network behaviour have not been verified
  beyond Chromium.
* Names are self-chosen and not enforced; the id suffix tells same-named players apart.
* The mesh is capped at 16 players.
* The game must stay in the foreground: a backgrounded tab stops sending and the peer times out
  after 5 s.
* The input delay is fixed at 6 ticks; it could be chosen from the measured round trip.
* No native desktop transport yet (the protocol and session are platform independent, so a desktop
  client needs only a `Transport` implementation).
