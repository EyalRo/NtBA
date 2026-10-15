# NtBA signalling service

A tiny Cloudflare Worker that lets two browsers find each other and set up a direct WebRTC
connection. It **never carries game traffic**: once the peers are connected they talk
directly and this service is out of the loop.

| Route | Purpose |
| --- | --- |
| `POST /rooms` | Create a room, returns `{ "code": "K7M2QX", "ttl": 600 }` |
| `GET /rooms/<code>/ws?role=host\|guest` | WebSocket to the room's Durable Object |
| `GET /ice` | `{ "iceServers": [...] }`: Cloudflare STUN, plus short-lived TURN credentials if configured |
| `GET /health` | Liveness |

## How a room works

One Durable Object per room code (SQLite-backed, free plan). It holds at most two WebSockets,
tagged `host` and `guest`, and forwards `offer`, `answer` and `ice` JSON messages verbatim to the
other peer. Messages are limited to 8 KiB and 100 per socket. It tells the host `peer-joined`
when the guest arrives and `peer-left` when either leaves. A peer sends `{"type":"done"}` once
connected to close the room early; otherwise an alarm deletes it after `ROOM_TTL_SECONDS`.
Nothing about players is stored, and signalling payloads and IP addresses are never logged.

Why a Durable Object and not a plain Worker: separate requests for the same room can be served
by different isolates, which do not share memory, so an in-memory room table is unreliable.

## Configure

`wrangler.toml` holds the non-secret settings: `ALLOWED_ORIGINS` (browser origins allowed to
use the service; add your own domain) and `ROOM_TTL_SECONDS`. Requests are rate limited per IP
with a Workers rate-limit binding.

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

Logs are one JSON object per line with a `level` of `DEBUG`, `INFO`, `WARN` or `ERROR`.
