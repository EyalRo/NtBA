# Signalling service

A tiny Cloudflare Worker that tells browsers who is online and lets them find each other and set up
direct WebRTC connections. It **never carries game, chat or challenge traffic**: once the peers are
connected they talk directly and this service is out of the loop.

| Route | Purpose |
| --- | --- |
| `POST /rooms` | Create a room, returns `{ "code": "K7M2QX", "ttl": 600 }` |
| `GET /rooms/<code>/ws?role=host\|guest` | WebSocket to the room's Durable Object |
| `GET /lobby/ws` | WebSocket to the global `Lobby` Durable Object (the online lobby) |
| `GET /ice?room=<code>` | `{ "iceServers": [...] }`: Cloudflare STUN, plus short-lived TURN credentials if configured **and** the room exists |
| `GET /health` | Liveness |

Source: [`signaling/`](https://github.com/EyalRo/NtBA/tree/main/signaling) in the repository. Design
and rationale: [Online Play](Online-Play).

## How a room works

One Durable Object per room code (SQLite-backed, free plan). It holds at most two WebSockets,
tagged `host` and `guest`, and forwards `offer`, `answer` and `ice` JSON messages verbatim to the
other peer. Messages are limited to 8 KiB and 100 per socket. It tells the host `peer-joined`
when the guest arrives and `peer-left` when either leaves. A peer sends `{"type":"done"}` once
connected to close the room early; otherwise an alarm deletes it after `ROOM_TTL_SECONDS`.
Nothing about players is stored, and signalling payloads and IP addresses are never logged.

Why a Durable Object and not a plain Worker: separate requests for the same room can be served
by different isolates, which do not share memory, so an in-memory room table is unreliable.

## How the lobby works

One global `Lobby` Durable Object (SQLite-backed, free plan; every request is routed to the instance
named `main`). It uses the WebSocket Hibernation API: the roster lives in the sockets' attachments,
so it survives hibernation and nothing is stored. It lists who is online and relays WebRTC
handshakes between players by id. It never sees chat.

| Direction | Message | Meaning |
| --- | --- | --- |
| client to server | `hello { id, nick }` | Join the roster (`id` is 12 letters and digits; `nick` is 1 to 20 characters). |
| client to server | `status { status }` | `lobby` or `in-match`. |
| client to server | `signal { to, data }` | Relay `data` to player `to`. |
| server to client | `welcome { you, roster }` | Joined; the current roster. |
| server to client | `joined`, `left`, `status` | Roster changes. |
| server to client | `signal { from, data }` | A relayed message. |
| server to client | `full` | The lobby already has 16 players. |
| server to client | `error { reason }` | A refused message. |

Limits: at most 16 players, whole messages up to 8 KB, 600 messages per minute per socket, and
duplicate ids are refused. A new connection gets 503 ("lobby busy") when 32 sockets are already
connected, and a socket that has not sent `hello` within 10 seconds is closed (checked when the next
connection arrives). The route uses the same origin allow-list and per-IP rate limit
(`LIMITER`) as the other WebSocket routes. Logs carry no nicknames or payloads. The client side is
described in [Online Play](Online-Play).

## Security model

There are **no accounts and no authentication on purpose**; the room code is the only secret
and a match is two anonymous peers. What that means and what limits the damage:

| Risk | Mitigation | Residual |
| --- | --- | --- |
| A website you do not control drives the service from visitors' browsers (CSRF, cross-site WebSocket hijacking) | Browsers always send `Origin`; anything outside `ALLOWED_ORIGINS` gets 403 before any work is done | `Origin` is not authentication: `curl` and scripts can omit it, so it only stops *browsers* on other sites |
| Guessing a room code to join someone else's match | 6 characters from a 31-symbol alphabet (about 887 million codes), codes live at most `ROOM_TTL_SECONDS` (10 min), one host and one guest per room, joins are rate limited | A code seen over your shoulder or pasted in a public chat can be used by anyone, once, before your friend joins; treat it like a one-time invite |
| Anonymous callers creating or probing many rooms (each is a Durable Object, so this costs money) | Separate per-IP limits: 6 room creations/min, 30 ICE lookups and joins/min; junk (non-WebSocket, bad role, bad code) is rejected in the Worker before any Durable Object is touched | The rate-limit API is per Cloudflare location and permissive, so a distributed flood is not stopped. Next steps if that ever happens: a Cloudflare Turnstile token on `POST /rooms`, and a billing alert on the account |
| Free relay bandwidth (TURN) for strangers | TURN credentials are only issued to a caller who names a room that currently exists (`/ice?room=CODE`), last one hour, and need the secrets that are off by default | Anyone who can create a room can obtain relay credentials for an hour; keep the account's Realtime usage alert on |
| Malicious peer | Room relays only `offer`, `answer`, `ice` JSON (8 KiB, 100 messages per socket), nothing else; the browser parses SDP/ICE as for any WebRTC app | A peer you connect to learns your IP address, as with any peer-to-peer game |
| Log injection or sensitive data in logs | Logs are JSON lines; attacker-controlled values are length-bounded; IPs and signalling payloads are never logged | |
| Spam or impersonation in the lobby | Whole messages capped at 8 KB and 600 per minute per socket, 16 players at most, duplicate ids refused, nicknames validated (control and bidi override characters rejected); the Lobby only relays handshakes and never sees chat, which clients limit and sanitise themselves | Names are self-chosen and not enforced; players in the lobby can see each other's IP address |
| Rate limiter missing | Fails open (the service still works) but logs a WARN on every request | Check `wrangler.toml` bindings after deploying |

## Configure

`wrangler.toml` holds the non-secret settings: `ALLOWED_ORIGINS` (browser origins allowed to
use the service; add your own domain) and `ROOM_TTL_SECONDS`. Requests are rate limited per IP
with Workers rate-limit bindings (`ROOM_LIMITER` for room creation, `LIMITER` for the rest).

The Worker needs two Durable Object bindings, `ROOMS` (class `Room`) and `LOBBY` (class `Lobby`),
and the migrations `v1` (`Room`) and `v2` (`Lobby`) declared in `wrangler.toml`.

TURN is optional. NAT pairs that cannot connect directly (symmetric NAT, some mobile networks)
need a relay. To enable Cloudflare's TURN service (its pricing includes a monthly free
allowance; check the current terms), create a TURN key in the dashboard and store it as secrets,
never in the repo:

```sh
npx wrangler secret put TURN_KEY_ID
npx wrangler secret put TURN_KEY_API_TOKEN
```

Without the secrets `/ice` returns STUN only.

## Develop and deploy

```sh
npm install
npm test            # unit tests (Node's built-in runner, no network)
npm run dev         # local workerd on http://127.0.0.1:8787
npm run e2e         # end-to-end check against the running dev server
npm run deploy      # needs `wrangler login` or CLOUDFLARE_API_TOKEN in your environment
```

The Worker is deployed at `https://ntba-signaling.isdino.workers.dev`, and the page's signalling URL
is `SIGNALING_URL` in `web/config.js`. Deploying needs the `LOBBY` binding and the `v2` migration in
`wrangler.toml`; `npm run deploy` applies them.

Logs are one JSON object per line with a `level` of `DEBUG`, `INFO`, `WARN` or `ERROR`.
`wrangler.toml` turns on Workers observability, so these lines are kept and can be searched under
Workers Logs in the Cloudflare dashboard once the Worker has been deployed with that setting.
