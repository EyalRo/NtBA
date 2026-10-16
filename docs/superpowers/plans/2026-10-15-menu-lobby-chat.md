# Main menu, online lobby and peer-to-peer chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a title menu (Local / Online), an online lobby listing everyone online with Hebrew-and-English peer-to-peer chat and click-to-challenge matchmaking, and a pixel "Development Version" badge in the game's top-right corner.

**Architecture:** Cloudflare only detects presence: one global `Lobby` Durable Object keeps a roster and relays WebRTC handshakes. Every lobby member opens a data channel to every other member (full mesh, cap 16) and chat/challenges travel over those channels. A challenge carries a fresh room code and, once accepted, both players run the existing `PeerLink` match handshake unchanged. The badge is drawn by the Rust renderer, outside the simulation.

**Tech Stack:** Rust (`ntba-core` renderer, wasm C ABI), plain ES modules in `web/` (no build step, no dependencies), Cloudflare Worker + Durable Object in `signaling/`, `node:test`.

**Spec:** `docs/superpowers/specs/2026-10-15-menu-lobby-chat-design.md`

## Global Constraints

- Cloudflare is used only for presence and relaying WebRTC handshakes; the Worker never sees a chat message, and chat is never stored.
- No accounts. Visible name is `nick#<3 chars of peer id>`. Nick is 1-20 characters, trimmed, no control characters. Peer id is 12 random characters, regenerated on every (re)connect.
- Lobby cap is 16 players. A `signal` payload (whole message) is at most 8 KB. Chat text is at most 500 characters.
- Outgoing chat is limited to 2 messages per second. Incoming messages above 5 per second from one peer are dropped. A challenge expires after 30 s.
- Chat and names are inserted into the DOM with `textContent` only, and chat text elements carry `dir="auto"`.
- The new Worker route uses the same origin allow-list and per-IP rate limit as the existing routes. Logs carry no nicknames and no payloads.
- The badge is drawn into the 320x200 framebuffer at the top-right using the core's own pixel font and palette, outside the simulation (game state and hashes unchanged).
- Local play must keep working when the Worker is unreachable.
- Repo style: no new dependencies; Rust must pass `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`; JS is ES modules.
- Run Rust and make targets through the nix shell: `nix develop -c <command>`. The Worker tests run with `npm test` in `signaling/`.
- Prerequisite for Part D only: PR #50 (host connect timer) is merged to `main`.

## Review Focus

These are inputs the spec implies but a straight reading of the tasks would not exercise. Each has a test in the task that owns the code.

1. **Hebrew, mixed-direction text and bidi-override characters** in nicknames and chat: Hebrew and mixed text must be accepted and shown intact, while invisible bidi override/isolate characters must be stripped from chat and rejected in nicknames. (Task 3 server nick tests, Task 6 client tests, Task 8 manual RTL check.)
2. **The 17th player:** must get a clear "lobby full" result, not a hang or an endless reconnect loop. (Task 4 server test, Task 7 client test.)
3. **A peer that can't be reached** (strict firewall or NAT): the roster must show them as unreachable, retry once, and chat to everyone else must keep working. (Task 7.)
4. **Both players challenge each other at the same moment:** both must end up cleanly cancelled with a message, with no stranded hosted room. (Task 9.)
5. **A flooding or oversized peer:** messages over the size limit are rejected and bursts above the rate are dropped without crashing the receiver. (Task 6, Task 7.)

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/ntba-core/src/render.rs` (modify) | Draw the "Development Version" badge last in `draw`. |
| `web/package.json` (create) | `{"type":"module"}` so Node treats `web/*.js` as ES modules for tests. |
| `web/flow.js` (create) | Pure screen state machine: title / local / online / match. |
| `web/lobby-protocol.js` (create) | Pure helpers: nick and chat cleaning, peer-message validation, offer-direction rule, rate limiter, id generation. |
| `web/lobby.js` (create) | Presence socket plus WebRTC mesh; emits roster, chat, peer messages. |
| `web/chat.js` (create) | `ChatView`: appends chat lines to the DOM safely. |
| `web/lobby-ui.js` (create) | `renderRoster`: draws the player list with mute and challenge buttons. |
| `web/challenge.js` (create) | Pure challenge reducer returning state plus effects. |
| `web/main.js` (modify) | Screen switching, lobby wiring, challenge effects. |
| `web/index.html`, `web/style.css` (modify) | Title menu, lobby panel, challenge box. |
| `web/test/*.test.js` (create) | Node tests for the pure modules and the `Lobby` class with fakes. |
| `signaling/src/lobby-logic.js` (create) | Pure message validation for the Lobby. |
| `signaling/src/lobby.js` (create) | The `Lobby` Durable Object. |
| `signaling/src/index.js`, `signaling/wrangler.toml` (modify) | Route `GET /lobby/ws`, binding, migration. |
| `signaling/test/lobby-logic.test.js`, `lobby.test.js` (create), `worker.test.js` (modify) | Worker tests. |
| `.github/workflows/ci.yml` (modify) | Run `node --test web/test/`. |
| `wiki/Online-Play.md`, `wiki/Roadmap.md` (modify) | Document the lobby. |

Branch per part, each from `main`: `feat/menu-badge`, `feat/lobby-presence`, `feat/lobby-chat`, `feat/lobby-challenges`.

---

# Part A: Menu and badge (PR 1)

### Task 1: The "Development Version" badge

**Files:**
- Modify: `crates/ntba-core/src/render.rs` (the `draw` function near line 298, `draw_overlays` near line 1075, tests module near line 1097)

**Interfaces:**
- Consumes: existing `Framebuffer::rect`, `Framebuffer::bevel`, `Framebuffer::text`, `text_width`, palette constants `BLACK`, `STEEL`, `LED`, and `SCREEN_W`.
- Produces: `fn dev_badge_rect() -> (i32, i32, i32, i32)` returning `(x, y, w, h)` of the badge plate, and `fn draw_dev_badge(fb: &mut Framebuffer)`. Nothing outside `render.rs` changes.

- [ ] **Step 0: Branch**

```bash
cd /home/stags/Source/NtBA
git switch -c feat/menu-badge main
```

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `crates/ntba-core/src/render.rs`:

```rust
    #[test]
    fn dev_badge_is_drawn_in_the_top_right_corner() {
        let g = Game::new(3);
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let (x0, y0, w, h) = dev_badge_rect();
        let sw = SCREEN_W as i32;
        assert_eq!(x0 + w, sw, "badge must be flush with the right edge");
        assert_eq!(y0, 0, "badge must be flush with the top edge");
        assert!(w <= 100 && h <= 12, "badge must stay a small corner plate");
        let lit = (y0..y0 + h)
            .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.idx[(y * sw + x) as usize] == LED)
            .count();
        assert!(lit >= 60, "badge text should light many pixels, got {lit}");
    }

    #[test]
    fn dev_badge_is_still_drawn_on_the_final_screen() {
        let mut g = Game::new(3);
        g.phase = Phase::Final; // the overlay returns early; the badge must not
        let mut fb = Framebuffer::new();
        draw(&g, &mut fb);
        let (x0, y0, w, h) = dev_badge_rect();
        let sw = SCREEN_W as i32;
        let lit = (y0..y0 + h)
            .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.idx[(y * sw + x) as usize] == LED)
            .count();
        assert!(lit >= 60, "got {lit}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `nix develop -c cargo test -p ntba-core dev_badge`
Expected: FAIL to compile with "cannot find function `dev_badge_rect`".

- [ ] **Step 3: Write the implementation**

In `render.rs`, directly above `#[cfg(test)]` (after `draw_overlays`) add:

```rust
const DEV_LABEL: &str = "DEVELOPMENT VERSION";

/// Where the "Development Version" plate sits: flush with the top-right corner.
fn dev_badge_rect() -> (i32, i32, i32, i32) {
    let w = text_width(DEV_LABEL, 1) + 6;
    (SCREEN_W as i32 - w, 0, w, 9)
}

/// The plate itself, in the same amber lamps and bevelled steel as the other overlay plates.
/// Drawn last so nothing covers it.
fn draw_dev_badge(fb: &mut Framebuffer) {
    let (x, y, w, h) = dev_badge_rect();
    fb.rect(x, y, w, h, BLACK);
    fb.bevel(x, y, w, h, STEEL[2], STEEL[0]);
    fb.text(x + 3, y + 2, DEV_LABEL, LED, 1);
}
```

In `draw`, change the tail so the badge is drawn after the overlays:

```rust
    draw_overlays(g, fb);
    draw_dev_badge(fb);
    fb.expand();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `nix develop -c cargo test --workspace`
Expected: all tests PASS, including both new ones. If a pre-existing test fails because it pinned frame pixels, the failure message names it; update that test's expectation only if it asserts the exact pixels the badge now covers (top-right 82x9 area), and say so in the commit message.

- [ ] **Step 5: Check formatting, lints and determinism**

```bash
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c make determinism
```
Expected: all succeed (determinism proves the simulation is unchanged).

- [ ] **Step 6: Look at it**

```bash
nix develop -c make serve   # leave running in another terminal; serves http://localhost:8080
```
Open `http://localhost:8080/` and confirm the badge reads "DEVELOPMENT VERSION" in amber on a steel plate at the top-right, and does not cover the scoreboard (top-left) or the centre message plate. If it overlaps a message plate (message plate is at y 8-22, badge is y 0-8), that is acceptable: the badge is drawn on top. Stop the server.

- [ ] **Step 7: Commit**

```bash
git add crates/ntba-core/src/render.rs
git commit -m "Render: add a pixel 'Development Version' badge in the top-right corner

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 2: Screen flow and the title menu

**Files:**
- Create: `web/package.json`, `web/flow.js`, `web/test/flow.test.js`
- Modify: `web/index.html`, `web/style.css`, `web/main.js`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: existing `main.js` (`showMenu`, `showBusy`, `endOnline`, `startOnline`, the `ui` object, `held`, `pending`).
- Produces: `nextScreen(screen, event) -> screen` in `web/flow.js`, with screens `'title' | 'local' | 'online' | 'match'` and events `'local' | 'online' | 'menu' | 'match-start' | 'match-end'`. In `main.js`: module-level `let screen`, and inside `main()` the functions `renderScreen()` and `go(event)` that later tasks extend.

- [ ] **Step 1: Write the failing test**

Create `web/package.json`:

```json
{ "private": true, "type": "module" }
```

Create `web/test/flow.test.js`:

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nextScreen } from '../flow.js';

test('the title leads to local or online', () => {
  assert.equal(nextScreen('title', 'local'), 'local');
  assert.equal(nextScreen('title', 'online'), 'online');
});

test('local and online go back to the title', () => {
  assert.equal(nextScreen('local', 'menu'), 'title');
  assert.equal(nextScreen('online', 'menu'), 'title');
});

test('a match starts from online and returns to online', () => {
  assert.equal(nextScreen('online', 'match-start'), 'match');
  assert.equal(nextScreen('match', 'match-end'), 'online');
});

test('events that do not apply leave the screen unchanged', () => {
  assert.equal(nextScreen('title', 'menu'), 'title');
  assert.equal(nextScreen('local', 'match-start'), 'local');
  assert.equal(nextScreen('online', 'match-end'), 'online');
  assert.equal(nextScreen('match', 'menu'), 'match');
  assert.equal(nextScreen('title', 'nonsense'), 'title');
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/test/`
Expected: FAIL with "Cannot find module '../flow.js'".

- [ ] **Step 3: Write `web/flow.js`**

```js
// Which screen the page is on. Pure, so it can be tested without a browser.
//   title  - idle demo behind the Local / Online buttons
//   local  - the local two-pad game
//   online - the lobby and room-code panel
//   match  - an online match is being played
export function nextScreen(screen, event) {
  switch (`${screen}:${event}`) {
    case 'title:local': return 'local';
    case 'title:online': return 'online';
    case 'local:menu': return 'title';
    case 'online:menu': return 'title';
    case 'online:match-start': return 'match';
    case 'match:match-end': return 'online';
    default: return screen;
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test web/test/`
Expected: PASS (4 tests).

- [ ] **Step 5: Add the markup**

In `web/index.html`, directly after the `<p id="status" role="status">Loading…</p>` line add:

```html
    <nav class="menu" id="menu" aria-label="Main menu" hidden>
      <button type="button" id="menu-local">Local game</button>
      <button type="button" id="menu-online">Online</button>
    </nav>
    <button type="button" class="menu-back" id="menu-back" hidden>Main menu</button>
```

- [ ] **Step 6: Add the styles**

Append to `web/style.css` (before the `@media` block):

```css
/* Title menu */
.menu { display: flex; gap: 16px; justify-content: center; flex-wrap: wrap; }
.menu button, .menu-back {
  font: inherit; font-size: 1.1rem; letter-spacing: .1em; text-transform: uppercase;
  color: #fff; background: var(--panel); border: 3px solid var(--line); border-radius: 4px;
  padding: 10px 24px; cursor: pointer; box-shadow: 0 4px 0 #000;
}
.menu button:hover, .menu button:focus-visible, .menu-back:hover, .menu-back:focus-visible {
  border-color: var(--gold); color: var(--gold); outline: none;
}
.menu[hidden], .menu-back[hidden], .menu button[hidden] { display: none; }
.menu-back { font-size: 14px; padding: 4px 12px; box-shadow: none; letter-spacing: 0; text-transform: none; }
```

- [ ] **Step 7: Wire `web/main.js`**

Make these edits.

(a) After `import { ConnectError, PeerLink } from './net.js';` add:

```js
import { nextScreen } from './flow.js';
```

(b) In the `ui` object, after the `text: document.getElementById('online-status'),` line add:

```js
  title: document.getElementById('menu'),
  titleLocal: document.getElementById('menu-local'),
  titleOnline: document.getElementById('menu-online'),
  back: document.getElementById('menu-back'),
```

(c) After `let held = 0;` add:

```js
let screen = 'title'; // 'title' | 'local' | 'online' | 'match', see flow.js
```

(d) Replace the first line of the `keydown` handler body

```js
  if (e.target instanceof HTMLInputElement) return; // typing a room code
```

with

```js
  if (e.target instanceof HTMLInputElement) return; // typing a room code
  if (screen !== 'local' && screen !== 'match') return; // title and lobby take no game input
```

(e) In `main()`, replace

```js
  wasm.ntba_init((Date.now() & 0xffffffff) >>> 0);
  status.textContent = '';
  canvas.focus();
```

with

```js
  const newSeed = () => (Date.now() & 0xffffffff) >>> 0;
  wasm.ntba_init(newSeed());
```

(f) At the end of `endOnline`, after `status.textContent = '';`, add:

```js
    go('match-end');
```

(g) In `startOnline`'s `onOpen`, after `online = true;` add:

```js
        go('match-start');
```

(h) Replace the whole `if (SIGNALING_URL && typeof RTCPeerConnection !== 'undefined') { ... }` block with:

```js
  const canOnline = Boolean(SIGNALING_URL) && typeof RTCPeerConnection !== 'undefined';
  if (canOnline) {
    ui.host.addEventListener('click', host);
    ui.form.addEventListener('submit', (e) => {
      e.preventDefault();
      const code = ui.code.value.toUpperCase().replace(/[\s-]/g, '');
      if (!CODE_RE.test(code)) {
        say('That code does not look right - it is 6 letters and digits.');
        return;
      }
      join(code);
    });
    ui.leave.addEventListener('click', () => {
      if (online) {
        wasm.ntba_net_stop();
        drainNet();
        endOnline(END_TEXT[2]);
      } else {
        const l = link;
        link = null;
        l?.close();
        showMenu('');
      }
    });
    showMenu();
  }

  // ---- screens -----------------------------------------------------------------------
  function renderScreen() {
    ui.title.hidden = screen !== 'title';
    ui.titleOnline.hidden = !canOnline;
    ui.back.hidden = screen !== 'local' && screen !== 'online';
    ui.panel.hidden = !(canOnline && (screen === 'online' || screen === 'match'));
    status.textContent = screen === 'title' ? 'Choose how to play' : '';
  }

  function go(event) {
    screen = nextScreen(screen, event);
    renderScreen();
    if (screen === 'local' || screen === 'match') canvas.focus();
  }

  ui.titleLocal.addEventListener('click', () => {
    wasm.ntba_init(newSeed()); // a fresh game; the idle demo behind the title is discarded
    held = 0;
    pending = 0;
    go('local');
  });
  ui.titleOnline.addEventListener('click', () => go('online'));
  ui.back.addEventListener('click', () => {
    if (screen === 'local') {
      wasm.ntba_init(newSeed());
      held = 0;
      pending = 0;
    }
    if (screen === 'online' && link) {
      const l = link;
      link = null;
      l.close();
    }
    go('menu');
    if (canOnline) showMenu('');
  });
  renderScreen();
```

Note `showMenu`/`showBusy` already exist above this block in `main()`; `go` and `renderScreen` are function declarations so `endOnline`/`startOnline` may call `go` before this point in the source.

- [ ] **Step 8: CI step**

In `.github/workflows/ci.yml`, in the `check` job, after the "Check the wasm online-play glue" step add:

```yaml
      - name: Test the web modules
        run: node --test web/test/
```

- [ ] **Step 9: Verify in the browser**

```bash
nix develop -c make serve
```
Open `http://localhost:8080/`. Check, in order: the page opens on a title with "Choose how to play" and two buttons, and the keyboard does nothing; **Local game** starts a game that responds to WASD; **Main menu** returns to the title; **Online** shows the host/join panel (it works exactly as before); **Main menu** from there returns to the title. Run `node --test web/test/` once more. Stop the server.

- [ ] **Step 10: Commit and open the PR**

```bash
git add web .github/workflows/ci.yml
git commit -m "Web: title menu (Local / Online) with a screen state machine

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git push -u origin feat/menu-badge
nix run nixpkgs#gh -- pr create --base main --title "Menu and Development Version badge" --body "Part 1 of the lobby spec (docs/superpowers/specs/2026-10-15-menu-lobby-chat-design.md): title menu with Local and Online, and a pixel badge in the top-right of the game screen.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

---

# Part B: Presence (PR 2)

### Task 3: Lobby message validation (pure)

**Files:**
- Create: `signaling/src/lobby-logic.js`, `signaling/test/lobby-logic.test.js`

**Interfaces:**
- Produces (used by Task 4): constants `MAX_PLAYERS = 16`, `MAX_MESSAGES_PER_MINUTE = 600`, `MAX_LOBBY_MESSAGE_BYTES = 8192`; `cleanNick(input) -> string | null`; `parseClientMessage(data) -> { kind: 'hello', id, nick } | { kind: 'status', status } | { kind: 'signal', to, data } | { kind: 'error', reason }`.

- [ ] **Step 0: Branch**

```bash
cd /home/stags/Source/NtBA
git switch -c feat/lobby-presence main
```

- [ ] **Step 1: Write the failing test**

Create `signaling/test/lobby-logic.test.js`:

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MAX_LOBBY_MESSAGE_BYTES, cleanNick, parseClientMessage } from '../src/lobby-logic.js';

const ID = 'abcDEF123456';
const msg = (o) => JSON.stringify(o);

test('cleanNick trims, collapses spaces and accepts Hebrew and English', () => {
  assert.equal(cleanNick('  Dana  '), 'Dana');
  assert.equal(cleanNick('a   b'), 'a b');
  assert.equal(cleanNick('  דנה  '), 'דנה');
  assert.equal(cleanNick('Dana דנה'), 'Dana דנה');
});

test('cleanNick enforces length by characters, not bytes', () => {
  assert.equal(cleanNick(''), null);
  assert.equal(cleanNick('   '), null);
  assert.equal(cleanNick('x'.repeat(20)), 'x'.repeat(20));
  assert.equal(cleanNick('x'.repeat(21)), null);
  assert.equal(cleanNick('🏀'.repeat(20)), '🏀'.repeat(20)); // 20 code points, 80 bytes
  assert.equal(cleanNick('🏀'.repeat(21)), null);
});

test('cleanNick rejects control and bidi override characters and non-strings', () => {
  assert.equal(cleanNick('a\u0000b'), null);
  assert.equal(cleanNick('a\nb'), null);
  assert.equal(cleanNick('evil\u202eeno'), null); // right-to-left override
  assert.equal(cleanNick('a\u2066b'), null); // isolate
  assert.equal(cleanNick(42), null);
  assert.equal(cleanNick(undefined), null);
});

test('hello is validated', () => {
  assert.deepEqual(parseClientMessage(msg({ type: 'hello', id: ID, nick: ' Dana ' })), {
    kind: 'hello', id: ID, nick: 'Dana',
  });
  assert.equal(parseClientMessage(msg({ type: 'hello', id: 'short', nick: 'Dana' })).reason, 'bad-id');
  assert.equal(parseClientMessage(msg({ type: 'hello', id: ID + '!', nick: 'Dana' })).reason, 'bad-id');
  assert.equal(parseClientMessage(msg({ type: 'hello', id: ID, nick: '' })).reason, 'bad-nick');
});

test('status is validated', () => {
  assert.deepEqual(parseClientMessage(msg({ type: 'status', status: 'in-match' })), { kind: 'status', status: 'in-match' });
  assert.equal(parseClientMessage(msg({ type: 'status', status: 'lobby' })).kind, 'status');
  assert.equal(parseClientMessage(msg({ type: 'status', status: 'away' })).reason, 'bad-status');
});

test('signal needs a target id and an object payload', () => {
  const ok = parseClientMessage(msg({ type: 'signal', to: ID, data: { kind: 'offer', sdp: 'v=0' } }));
  assert.deepEqual(ok, { kind: 'signal', to: ID, data: { kind: 'offer', sdp: 'v=0' } });
  assert.equal(parseClientMessage(msg({ type: 'signal', to: 'x', data: {} })).reason, 'bad-target');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: 'str' })).reason, 'bad-signal');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: [1] })).reason, 'bad-signal');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: null })).reason, 'bad-signal');
});

test('malformed input is rejected with a reason', () => {
  assert.equal(parseClientMessage(new ArrayBuffer(4)).reason, 'binary-not-allowed');
  assert.equal(parseClientMessage('{nope').reason, 'invalid-json');
  assert.equal(parseClientMessage('[]').reason, 'invalid-message');
  assert.equal(parseClientMessage('null').reason, 'invalid-message');
  assert.equal(parseClientMessage(msg({ type: 'chat', text: 'hi' })).reason, 'unknown-type');
  const big = msg({ type: 'signal', to: ID, data: { sdp: 'x'.repeat(MAX_LOBBY_MESSAGE_BYTES) } });
  assert.equal(parseClientMessage(big).reason, 'message-too-large');
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (from `signaling/`): `node --test test/lobby-logic.test.js`
Expected: FAIL with "Cannot find module '../src/lobby-logic.js'".

- [ ] **Step 3: Write `signaling/src/lobby-logic.js`**

```js
// Pure helpers for the Lobby Durable Object (no runtime APIs, so plain Node can test them).

export const MAX_PLAYERS = 16;
export const MAX_NICK_CHARS = 20;
export const MAX_LOBBY_MESSAGE_BYTES = 8 * 1024;
export const MAX_MESSAGES_PER_MINUTE = 600;

const STATUSES = new Set(['lobby', 'in-match']);
const ID_RE = /^[A-Za-z0-9]{12}$/;
// Control characters, plus the bidi override and isolate characters that can make one name
// read as another. Hebrew and English letters are fine.
const BAD_NICK_CHARS = /[\u0000-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069]/;

/** Trimmed, single-spaced nickname of 1-20 characters, or null if it is not acceptable. */
export function cleanNick(input) {
  if (typeof input !== 'string') return null;
  const nick = input.trim().replace(/\s+/g, ' ');
  if (BAD_NICK_CHARS.test(nick)) return null;
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run (from `signaling/`): `node --test test/lobby-logic.test.js`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add signaling/src/lobby-logic.js signaling/test/lobby-logic.test.js
git commit -m "Signalling: lobby message validation

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 4: The `Lobby` Durable Object

**Files:**
- Create: `signaling/src/lobby.js`, `signaling/test/lobby.test.js`

**Interfaces:**
- Consumes: `parseClientMessage`, `MAX_PLAYERS`, `MAX_MESSAGES_PER_MINUTE` from `lobby-logic.js` (Task 3); `log` from `logic.js`.
- Produces: `class Lobby` with `fetch(request)` (WebSocket upgrade), `webSocketMessage(ws, data)`, `webSocketClose(ws)`, `webSocketError(ws)`. Server-to-client messages: `welcome {you, roster}`, `joined {peer}`, `left {id}`, `status {id, status}`, `signal {from, data}`, `full {}`, `error {reason, to?}`. A roster entry is `{id, nick, status}`.

- [ ] **Step 1: Write the failing test**

Create `signaling/test/lobby.test.js`:

```js
import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { MAX_MESSAGES_PER_MINUTE, MAX_PLAYERS } from '../src/lobby-logic.js';
import { Lobby } from '../src/lobby.js';

class FakeSocket {
  constructor() {
    this.sent = [];
    this.closed = null;
    this.attachment = { joined: false, windowStart: Date.now(), count: 0 };
  }
  send(text) {
    if (this.closed) throw new Error('socket closed');
    this.sent.push(JSON.parse(text));
  }
  close(code, reason) {
    this.closed = { code, reason };
  }
  serializeAttachment(v) {
    this.attachment = structuredClone(v);
  }
  deserializeAttachment() {
    return structuredClone(this.attachment);
  }
}

class FakeCtx {
  constructor() {
    this.sockets = [];
  }
  acceptWebSocket(ws) {
    this.sockets.push(ws);
  }
  getWebSockets() {
    return this.sockets.filter((s) => !s.closed);
  }
}

let ctx;
let lobby;
beforeEach(() => {
  ctx = new FakeCtx();
  lobby = new Lobby(ctx);
  console.log = () => {};
});

const idOf = (n) => String(n).padStart(12, 'x');
const hello = (id, nick = 'Dana') => JSON.stringify({ type: 'hello', id, nick });
const types = (ws) => ws.sent.map((m) => m.type);

async function connect(id, nick) {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, hello(id, nick));
  return ws;
}

test('hello gets a welcome with the roster including the newcomer', async () => {
  const a = await connect(idOf(1), 'Ann');
  assert.deepEqual(a.sent[0], {
    type: 'welcome',
    you: { id: idOf(1), nick: 'Ann', status: 'lobby' },
    roster: [{ id: idOf(1), nick: 'Ann', status: 'lobby' }],
  });
});

test('existing players are told about a newcomer and the newcomer sees them', async () => {
  const a = await connect(idOf(1), 'Ann');
  const b = await connect(idOf(2), 'בן');
  assert.deepEqual(a.sent.at(-1), { type: 'joined', peer: { id: idOf(2), nick: 'בן', status: 'lobby' } });
  assert.equal(b.sent[0].roster.length, 2);
});

test('the 17th player gets full and is closed, and does not appear in the roster', async () => {
  const sockets = [];
  for (let i = 1; i <= MAX_PLAYERS; i++) sockets.push(await connect(idOf(i)));
  const extra = await connect(idOf(99));
  assert.deepEqual(extra.sent, [{ type: 'full' }]);
  assert.ok(extra.closed);
  assert.equal(sockets[0].sent.some((m) => m.type === 'joined' && m.peer.id === idOf(99)), false);
});

test('a duplicate id is refused and closed', async () => {
  await connect(idOf(1));
  const dup = await connect(idOf(1), 'Imposter');
  assert.deepEqual(dup.sent, [{ type: 'error', reason: 'duplicate-id' }]);
  assert.ok(dup.closed);
});

test('anything before hello is refused, and hello cannot be sent twice', async () => {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, JSON.stringify({ type: 'status', status: 'lobby' }));
  assert.deepEqual(ws.sent, [{ type: 'error', reason: 'hello-first' }]);
  await lobby.webSocketMessage(ws, hello(idOf(1)));
  await lobby.webSocketMessage(ws, hello(idOf(2)));
  assert.equal(ws.sent.at(-1).reason, 'already-joined');
});

test('a bad nickname is an error, not a join', async () => {
  const ws = new FakeSocket();
  ctx.acceptWebSocket(ws);
  await lobby.webSocketMessage(ws, hello(idOf(1), 'evil\u202ename'));
  assert.deepEqual(ws.sent, [{ type: 'error', reason: 'bad-nick' }]);
});

test('status changes are broadcast to the others', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'status', status: 'in-match' }));
  assert.deepEqual(b.sent.at(-1), { type: 'status', id: idOf(1), status: 'in-match' });
  assert.equal(types(a).includes('status'), false);
});

test('signal is forwarded to the target with from added', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'signal', to: idOf(2), data: { kind: 'offer', sdp: 's' } }));
  assert.deepEqual(b.sent.at(-1), { type: 'signal', from: idOf(1), data: { kind: 'offer', sdp: 's' } });
});

test('signal to someone who is not here is an error to the sender only', async () => {
  const a = await connect(idOf(1));
  await lobby.webSocketMessage(a, JSON.stringify({ type: 'signal', to: idOf(7), data: {} }));
  assert.deepEqual(a.sent.at(-1), { type: 'error', reason: 'unknown-peer', to: idOf(7) });
});

test('closing tells the others who left', async () => {
  const a = await connect(idOf(1));
  const b = await connect(idOf(2));
  a.close(1000, 'bye');
  await lobby.webSocketClose(a);
  assert.deepEqual(b.sent.at(-1), { type: 'left', id: idOf(1) });
});

test('an unjoined socket closing says nothing', async () => {
  const b = await connect(idOf(2));
  const ghost = new FakeSocket();
  ctx.acceptWebSocket(ghost);
  ghost.close(1000, 'x');
  await lobby.webSocketClose(ghost);
  assert.equal(types(b).includes('left'), false);
});

test('a socket that floods is closed', async () => {
  const a = await connect(idOf(1));
  for (let i = 0; i < MAX_MESSAGES_PER_MINUTE + 5 && !a.closed; i++) {
    await lobby.webSocketMessage(a, JSON.stringify({ type: 'status', status: 'lobby' }));
  }
  assert.deepEqual(a.closed, { code: 1008, reason: 'too many messages' });
});

test('malformed messages get an error and the socket stays open', async () => {
  const a = await connect(idOf(1));
  await lobby.webSocketMessage(a, '{nope');
  assert.deepEqual(a.sent.at(-1), { type: 'error', reason: 'invalid-json' });
  assert.equal(a.closed, null);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (from `signaling/`): `node --test test/lobby.test.js`
Expected: FAIL with "Cannot find module '../src/lobby.js'".

- [ ] **Step 3: Write `signaling/src/lobby.js`**

```js
import { MAX_MESSAGES_PER_MINUTE, MAX_PLAYERS, parseClientMessage } from './lobby-logic.js';
import { log } from './logic.js';

// One global Durable Object: who is online, and a relay for the WebRTC handshakes between them.
// It keeps nothing but the roster, held in each socket's attachment so it survives hibernation,
// and it never sees chat: players talk to each other directly.
export class Lobby {
  constructor(ctx) {
    this.ctx = ctx;
  }

  async fetch(request) {
    if (request.headers.get('Upgrade') !== 'websocket') {
      return new Response('expected a WebSocket upgrade', { status: 426 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ joined: false, windowStart: Date.now(), count: 0 });
    return new Response(null, { status: 101, webSocket: client });
  }

  roster() {
    return this.ctx
      .getWebSockets()
      .map((ws) => ws.deserializeAttachment())
      .filter((a) => a?.joined)
      .map(({ id, nick, status }) => ({ id, nick, status }));
  }

  send(ws, obj) {
    try {
      ws.send(JSON.stringify(obj));
    } catch {
      // Already gone; its close handler cleans up.
    }
  }

  broadcast(except, obj) {
    for (const ws of this.ctx.getWebSockets()) {
      if (ws !== except && ws.deserializeAttachment()?.joined) this.send(ws, obj);
    }
  }

  async webSocketMessage(ws, data) {
    const att = ws.deserializeAttachment();
    const now = Date.now();
    if (now - att.windowStart > 60_000) {
      att.windowStart = now;
      att.count = 0;
    }
    att.count += 1;
    ws.serializeAttachment(att);
    if (att.count > MAX_MESSAGES_PER_MINUTE) {
      log('WARN', 'lobby socket flooding');
      ws.close(1008, 'too many messages');
      return;
    }

    const result = parseClientMessage(data);
    if (result.kind === 'error') {
      log('WARN', 'rejected lobby message', { reason: result.reason });
      this.send(ws, { type: 'error', reason: result.reason });
      return;
    }

    if (result.kind === 'hello') {
      if (att.joined) {
        this.send(ws, { type: 'error', reason: 'already-joined' });
        return;
      }
      const roster = this.roster();
      if (roster.some((p) => p.id === result.id)) {
        this.send(ws, { type: 'error', reason: 'duplicate-id' });
        ws.close(1008, 'duplicate id');
        return;
      }
      if (roster.length >= MAX_PLAYERS) {
        log('INFO', 'lobby full');
        this.send(ws, { type: 'full' });
        ws.close(1013, 'lobby full');
        return;
      }
      const me = { id: result.id, nick: result.nick, status: 'lobby' };
      ws.serializeAttachment({ ...att, joined: true, ...me });
      this.send(ws, { type: 'welcome', you: me, roster: [...roster, me] });
      this.broadcast(ws, { type: 'joined', peer: me });
      log('INFO', 'lobby joined', { players: roster.length + 1 });
      return;
    }

    if (!att.joined) {
      this.send(ws, { type: 'error', reason: 'hello-first' });
      return;
    }

    if (result.kind === 'status') {
      ws.serializeAttachment({ ...att, status: result.status });
      this.broadcast(ws, { type: 'status', id: att.id, status: result.status });
      return;
    }

    // signal: forward to the named player, naming the sender.
    const target = this.ctx
      .getWebSockets()
      .find((s) => s.deserializeAttachment()?.joined && s.deserializeAttachment().id === result.to);
    if (!target) {
      this.send(ws, { type: 'error', reason: 'unknown-peer', to: result.to });
      return;
    }
    this.send(target, { type: 'signal', from: att.id, data: result.data });
  }

  async webSocketClose(ws) {
    this.left(ws);
  }

  async webSocketError(ws) {
    this.left(ws);
  }

  left(ws) {
    const att = ws.deserializeAttachment();
    if (!att?.joined) return;
    log('INFO', 'lobby left');
    this.broadcast(ws, { type: 'left', id: att.id });
  }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (from `signaling/`): `node --test test/lobby.test.js`
Expected: PASS (12 tests). Then `npm test` to confirm nothing else broke.

- [ ] **Step 5: Commit**

```bash
git add signaling/src/lobby.js signaling/test/lobby.test.js
git commit -m "Signalling: Lobby Durable Object (roster and handshake relay)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 5: Route, binding and deployment

**Files:**
- Modify: `signaling/src/index.js`, `signaling/wrangler.toml`, `signaling/test/worker.test.js`

**Interfaces:**
- Consumes: `Lobby` (Task 4), existing `limited`, `json`, `originAllowed` in `index.js`.
- Produces: `GET /lobby/ws` (WebSocket upgrade only), served by the single Lobby object `idFromName('main')`; `env.LOBBY` binding; migration `v2` for the `Lobby` class.

- [ ] **Step 1: Write the failing tests**

In `signaling/test/worker.test.js`, extend `makeEnv` by adding a `LOBBY` entry next to `ROOMS` (inside the returned object, before `...extra`):

```js
    LOBBY: {
      idFromName: (name) => name,
      get: (id) => ({
        fetch: async (req) => {
          forwarded.push({ id, path: new URL(req.url).pathname });
          return new Response('lobby-proxied', { status: 200 });
        },
      }),
    },
```

Append these tests to the file:

```js
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `signaling/`): `node --test test/worker.test.js`
Expected: the lobby tests FAIL (404 instead of 426/200).

- [ ] **Step 3: Implement the route**

In `signaling/src/index.js`: change `import { Room } from './room.js';` to

```js
import { Lobby } from './lobby.js';
import { Room } from './room.js';
```

change `export { Room };` to `export { Lobby, Room };`, and insert before the line `const code = parseRoomWsPath(url.pathname);`:

```js
    if (url.pathname === '/lobby/ws' && request.method === 'GET') {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return json(request, env, { error: 'websocket-required' }, 426);
      }
      if (await limited(request, env, 'lobby')) return json(request, env, { error: 'rate-limited' }, 429);
      return env.LOBBY.get(env.LOBBY.idFromName('main')).fetch(request);
    }

```

- [ ] **Step 4: Add the binding and migration**

In `signaling/wrangler.toml`, after the existing `ROOMS` binding block add:

```toml
[[durable_objects.bindings]]
name = "LOBBY"
class_name = "Lobby"
```

and after the existing `[[migrations]]` block (tag `v1`) add:

```toml
[[migrations]]
tag = "v2"
new_sqlite_classes = ["Lobby"]
```

- [ ] **Step 5: Run the tests to verify they pass**

Run (from `signaling/`): `npm test`
Expected: all PASS.

- [ ] **Step 6: Commit and open the PR**

```bash
git add signaling
git commit -m "Signalling: serve the lobby at /lobby/ws

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git push -u origin feat/lobby-presence
nix run nixpkgs#gh -- pr create --base main --title "Lobby presence service (Worker + Durable Object)" --body "Part 2 of the lobby spec. A global Lobby Durable Object holds the roster and relays WebRTC handshakes between players; it never sees chat. Needs a deploy (\`npm run deploy\` in signaling/) before the client work can be tried live.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

- [ ] **Step 7: Deploy (owner action)**

The owner runs `cd signaling && npm run deploy` (needs their Cloudflare login; this environment has no credentials). Then check:

```bash
curl -s -o /dev/null -w "%{http_code}\n" https://ntba-signaling.isdino.workers.dev/lobby/ws          # 426
curl -s -o /dev/null -w "%{http_code}\n" -H "Origin: https://evil.example" -H "Upgrade: websocket" https://ntba-signaling.isdino.workers.dev/lobby/ws   # 403
```
Expected: `426` then `403`. Deploy only after the PR is merged, so the live Worker matches `main`.

---

# Part C: Peer mesh and chat (PR 3)

### Task 6: Lobby protocol helpers (pure)

**Files:**
- Create: `web/lobby-protocol.js`, `web/test/lobby-protocol.test.js`

**Interfaces:**
- Produces (used by Tasks 7, 8, 9): `MAX_CHAT_CHARS = 500`; `cleanNick(input) -> string|null`; `cleanChat(text) -> string|null`; `displayName(nick, id) -> string` (`"Dana#4F2"`); `shouldOffer(myId, theirId) -> boolean`; `parsePeerMessage(text) -> {type:'chat',text}|{type:'challenge',room}|{type:'accept'}|{type:'decline'}|{type:'cancel'}|null`; `createRateLimiter({perSecond, burst, now}) -> {take(): boolean}`; `randomId(getRandomValues?) -> 12 chars [A-Za-z0-9]`.

- [ ] **Step 0: Branch**

```bash
cd /home/stags/Source/NtBA
git switch -c feat/lobby-chat main
```
(If `feat/menu-badge` is not merged yet, branch from it instead: Part C edits `web/main.js` from Task 2.)

- [ ] **Step 1: Write the failing test**

Create `web/test/lobby-protocol.test.js`:

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  MAX_CHAT_CHARS,
  cleanChat,
  cleanNick,
  createRateLimiter,
  displayName,
  parsePeerMessage,
  randomId,
  shouldOffer,
} from '../lobby-protocol.js';
import { cleanNick as serverCleanNick } from '../../signaling/src/lobby-logic.js';

test('displayName appends the first three id characters in capitals', () => {
  assert.equal(displayName('Dana', 'abcdef123456'), 'Dana#ABC');
  assert.equal(displayName('דנה', 'x9y000000000'), 'דנה#X9Y');
});

test('the lower id makes the offer', () => {
  assert.equal(shouldOffer('AAAAAAAAAAAA', 'BBBBBBBBBBBB'), true);
  assert.equal(shouldOffer('BBBBBBBBBBBB', 'AAAAAAAAAAAA'), false);
});

test('cleanChat keeps Hebrew, English and mixed text intact', () => {
  assert.equal(cleanChat('שלום עולם'), 'שלום עולם');
  assert.equal(cleanChat('hello world'), 'hello world');
  assert.equal(cleanChat('  נוכל לשחק? let us play  '), 'נוכל לשחק? let us play');
});

test('cleanChat strips bidi overrides and control characters, and rejects empty text', () => {
  assert.equal(cleanChat('hi\u202ethere'), 'hithere');
  assert.equal(cleanChat('a\u0000b\u0007c'), 'abc');
  assert.equal(cleanChat('   '), null);
  assert.equal(cleanChat('\u202e\u202e'), null);
  assert.equal(cleanChat(42), null);
});

test('cleanChat limits length by characters', () => {
  assert.equal(cleanChat('x'.repeat(MAX_CHAT_CHARS)), 'x'.repeat(MAX_CHAT_CHARS));
  assert.equal(cleanChat('x'.repeat(MAX_CHAT_CHARS + 1)), null);
  assert.equal(cleanChat('🏀'.repeat(MAX_CHAT_CHARS)), '🏀'.repeat(MAX_CHAT_CHARS));
});

test('the client and server agree on which nicknames are acceptable', () => {
  const samples = ['Dana', '  דנה ', '', ' ', 'x'.repeat(20), 'x'.repeat(21), '🏀🏀', 'a\u0000', 'a\u202eb', 'a\u2067b', 'a  b', '\t'];
  for (const s of samples) assert.equal(cleanNick(s), serverCleanNick(s), JSON.stringify(s));
});

test('parsePeerMessage accepts the five message shapes', () => {
  assert.deepEqual(parsePeerMessage('{"type":"chat","text":"שלום"}'), { type: 'chat', text: 'שלום' });
  assert.deepEqual(parsePeerMessage('{"type":"challenge","room":"K7M2QX"}'), { type: 'challenge', room: 'K7M2QX' });
  assert.deepEqual(parsePeerMessage('{"type":"accept"}'), { type: 'accept' });
  assert.deepEqual(parsePeerMessage('{"type":"decline"}'), { type: 'decline' });
  assert.deepEqual(parsePeerMessage('{"type":"cancel"}'), { type: 'cancel' });
});

test('parsePeerMessage rejects everything else', () => {
  for (const bad of [
    '', 'nope', '[]', 'null', '42', '{}',
    '{"type":"chat"}', '{"type":"chat","text":""}', '{"type":"chat","text":5}',
    '{"type":"challenge"}', '{"type":"challenge","room":"short"}', '{"type":"challenge","room":"ABCDE0"}',
    '{"type":"launch-missiles"}',
  ]) {
    assert.equal(parsePeerMessage(bad), null, bad);
  }
  assert.equal(parsePeerMessage(new ArrayBuffer(3)), null);
});

test('parsePeerMessage rejects oversized messages and cleans chat text', () => {
  const big = JSON.stringify({ type: 'chat', text: 'x'.repeat(5000) });
  assert.equal(parsePeerMessage(big), null);
  assert.deepEqual(parsePeerMessage('{"type":"chat","text":"a\\u202Eb"}'), { type: 'chat', text: 'ab' });
});

test('the rate limiter allows a burst, then refills over time', () => {
  let t = 0;
  const limiter = createRateLimiter({ perSecond: 2, burst: 2, now: () => t });
  assert.equal(limiter.take(), true);
  assert.equal(limiter.take(), true);
  assert.equal(limiter.take(), false);
  t = 400; // 0.8 token
  assert.equal(limiter.take(), false);
  t = 500; // 1 token
  assert.equal(limiter.take(), true);
  assert.equal(limiter.take(), false);
});

test('randomId makes 12 alphanumeric characters and is not constant', () => {
  const a = randomId();
  const b = randomId();
  assert.match(a, /^[A-Za-z0-9]{12}$/);
  assert.notEqual(a, b);
});

test('randomId uses only unbiased bytes', () => {
  let calls = 0;
  const id = randomId((arr) => {
    calls += 1;
    arr.fill(calls === 1 ? 255 : 0); // 255 is rejected (biased); 0 maps to the first character
    return arr;
  });
  assert.equal(id, 'AAAAAAAAAAAA');
  assert.ok(calls >= 2);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/test/`
Expected: FAIL with "Cannot find module '../lobby-protocol.js'".

- [ ] **Step 3: Write `web/lobby-protocol.js`**

```js
// Pure helpers for the lobby: names, chat and peer-message validation, the offer rule, a rate
// limiter and id generation. No DOM and no network, so Node can test all of it.

export const MAX_CHAT_CHARS = 500;
export const MAX_NICK_CHARS = 20;
const MAX_PEER_MESSAGE_BYTES = 2048;
const ROOM_RE = /^[A-HJKMNP-Z2-9]{6}$/;
const ID_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';

// Control characters, plus the bidi override and isolate characters that can disguise text.
// Hebrew and English letters are untouched.
const BAD_CHARS = /[\u0000-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069]/;
const BAD_CHARS_G = new RegExp(BAD_CHARS.source, 'g');

/** Same rules as signaling/src/lobby-logic.js `cleanNick`; a test keeps the two in step. */
export function cleanNick(input) {
  if (typeof input !== 'string') return null;
  const nick = input.trim().replace(/\s+/g, ' ');
  if (BAD_CHARS.test(nick)) return null;
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test web/test/`
Expected: PASS (all tests in `web/test/`).

- [ ] **Step 5: Commit**

```bash
git add web/lobby-protocol.js web/test/lobby-protocol.test.js
git commit -m "Web: lobby protocol helpers (names, chat, peer messages, rate limit)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 7: The `Lobby` mesh client

**Files:**
- Create: `web/lobby.js`, `web/test/lobby.test.js`

**Interfaces:**
- Consumes: everything from `lobby-protocol.js` (Task 6); the server messages from Task 4.
- Produces: `class Lobby` with constructor `(signalingUrl, nick, handlers, deps = {})`, methods `connect()`, `chat(text) -> 'ok'|'empty'|'rate'`, `sendTo(id, msg) -> boolean`, `setStatus(status)`, `mute(id, on)`, `retryNow()`, `close()`. Handlers: `onState({phase})` with phase `'connecting'|'online'|'reconnecting'|'full'`; `onRoster(players)` where each player is `{id, nick, name, status, link, muted}` and `link` is `'self'|'open'|'connecting'|'unreachable'`; `onChat({from:{id,name}, text, mine})`; `onPeerMessage(id, msg)` for non-chat messages; `onPeerLeft(id)`. `deps` may override `WebSocket`, `RTCPeerConnection`, `setTimeout`, `clearTimeout`, `now`, `randomId` (for tests).

- [ ] **Step 1: Write the failing test**

Create `web/test/lobby.test.js`:

```js
import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { Lobby } from '../lobby.js';

const ME = 'MMMMMMMMMMMM';
const LOW = 'AAAAAAAAAAAA'; // sorts before ME: it makes the offer to us
const HIGH = 'ZZZZZZZZZZZZ'; // sorts after ME: we make the offer to it

class FakeWS {
  static all = [];
  constructor(url) {
    this.url = url;
    this.sent = [];
    this.readyState = 1;
    FakeWS.all.push(this);
  }
  send(t) {
    this.sent.push(JSON.parse(t));
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  push(obj) {
    this.onmessage?.({ data: JSON.stringify(obj) });
  }
}
class FakeDC {
  constructor() {
    this.readyState = 'connecting';
    this.sent = [];
  }
  send(t) {
    this.sent.push(JSON.parse(t));
  }
  close() {
    this.readyState = 'closed';
  }
  open() {
    this.readyState = 'open';
    this.onopen?.();
  }
}
class FakePC {
  static all = [];
  constructor() {
    this.offers = 0;
    this.remoteDescription = null;
    this.dc = null;
    FakePC.all.push(this);
  }
  createDataChannel() {
    this.dc = new FakeDC();
    return this.dc;
  }
  async createOffer() {
    this.offers += 1;
    return { type: 'offer', sdp: 'o' };
  }
  async createAnswer() {
    return { type: 'answer', sdp: 'a' };
  }
  async setLocalDescription(d) {
    this.localDescription = d;
  }
  async setRemoteDescription(d) {
    this.remoteDescription = d;
  }
  async addIceCandidate() {}
  close() {}
}

let timers;
let ids;
let events;
let lobby;
const tick = () => new Promise((r) => setImmediate(r));
const lastRoster = () => events.rosters.at(-1);
const link = (id) => lastRoster().find((p) => p.id === id)?.link;
const pcFor = (index) => FakePC.all[index];

function make() {
  FakeWS.all = [];
  FakePC.all = [];
  timers = [];
  ids = [ME, 'NNNNNNNNNNNN', 'OOOOOOOOOOOO'];
  events = { rosters: [], chats: [], states: [], peerMessages: [], left: [] };
  lobby = new Lobby(
    'https://signal.example',
    'Dana',
    {
      onState: (s) => events.states.push(s.phase),
      onRoster: (r) => events.rosters.push(r),
      onChat: (c) => events.chats.push(c),
      onPeerMessage: (id, m) => events.peerMessages.push([id, m]),
      onPeerLeft: (id) => events.left.push(id),
    },
    {
      WebSocket: FakeWS,
      RTCPeerConnection: FakePC,
      setTimeout: (fn, ms) => (timers.push({ fn, ms }), timers.length),
      clearTimeout: () => {},
      now: () => 0,
      randomId: () => ids.shift(),
    },
  );
  lobby.connect();
  FakeWS.all[0].onopen();
}

const welcome = (...others) => ({
  type: 'welcome',
  you: { id: ME, nick: 'Dana', status: 'lobby' },
  roster: [{ id: ME, nick: 'Dana', status: 'lobby' }, ...others.map((id) => ({ id, nick: id.slice(0, 3), status: 'lobby' }))],
});

beforeEach(() => {
  console.warn = () => {};
  make();
});

test('connect sends hello and welcome builds a roster with self first', async () => {
  const ws = FakeWS.all[0];
  assert.equal(ws.url, 'wss://signal.example/lobby/ws');
  assert.deepEqual(ws.sent[0], { type: 'hello', id: ME, nick: 'Dana' });
  ws.push(welcome(LOW, HIGH));
  await tick();
  const roster = lastRoster();
  assert.equal(roster[0].link, 'self');
  assert.equal(roster[0].name, 'Dana#MMM');
  assert.deepEqual(roster.slice(1).map((p) => p.id).sort(), [LOW, HIGH]);
  assert.equal(events.states.at(-1), 'online');
});

test('only the lower id makes the offer', async () => {
  FakeWS.all[0].push(welcome(LOW, HIGH));
  await tick();
  const offered = FakePC.all.filter((pc) => pc.offers > 0);
  assert.equal(offered.length, 1);
  const sentOffer = FakeWS.all[0].sent.find((m) => m.type === 'signal' && m.data.kind === 'offer');
  assert.equal(sentOffer.to, HIGH);
});

test('a data channel opening marks the player connected, and chat goes only to open peers', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  const [pcLow, pcHigh] = FakePC.all; // created in roster order
  pcLow.dc.open();
  assert.equal(link(LOW), 'open');
  assert.equal(link(HIGH), 'connecting');
  assert.equal(lobby.chat('שלום'), 'ok');
  assert.deepEqual(pcLow.dc.sent, [{ type: 'chat', text: 'שלום' }]);
  assert.deepEqual(pcHigh.dc.sent, []);
  assert.deepEqual(events.chats.at(-1), { from: { id: ME, name: 'Dana#MMM' }, text: 'שלום', mine: true });
});

test('an unreachable peer is marked, retried once, and chat to others keeps working', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  const [pcLow, pcHigh] = FakePC.all;
  pcLow.dc.open();
  // The connect timeout for HIGH fires: it never connected.
  const connectTimers = timers.filter((t) => t.ms === 20_000);
  assert.equal(connectTimers.length, 2);
  connectTimers[1].fn();
  assert.equal(link(HIGH), 'unreachable');
  assert.equal(link(LOW), 'open');
  assert.equal(lobby.chat('still here'), 'ok');
  assert.equal(pcLow.dc.sent.at(-1).text, 'still here');
  // We are the offerer for HIGH, so we retry once after a pause.
  const retry = timers.find((t) => t.ms === 2000);
  assert.ok(retry, 'a retry should be scheduled');
  retry.fn();
  assert.equal(link(HIGH), 'connecting');
  assert.equal(FakePC.all.length, 3);
  assert.notEqual(FakePC.all[2], pcHigh);
  // A second failure is not retried again.
  timers.filter((t) => t.ms === 20_000).at(-1).fn();
  assert.equal(link(HIGH), 'unreachable');
  assert.equal(timers.filter((t) => t.ms === 2000).length, 1);
});

test('the 17th player is told the lobby is full and does not reconnect in a loop', async () => {
  const ws = FakeWS.all[0];
  ws.push({ type: 'full' });
  ws.close();
  assert.equal(events.states.at(-1), 'full');
  assert.equal(timers.some((t) => t.ms <= 15_000 && t.ms >= 1000), false);
  assert.equal(FakeWS.all.length, 1);
});

test('losing the presence socket reconnects with a new id', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.close();
  assert.equal(events.states.at(-1), 'reconnecting');
  const reconnect = timers.find((t) => t.ms === 1000);
  assert.ok(reconnect);
  reconnect.fn();
  assert.equal(FakeWS.all.length, 2);
  FakeWS.all[1].onopen();
  assert.equal(FakeWS.all[1].sent[0].id, 'NNNNNNNNNNNN');
});

test('a flooding peer is dropped after the burst, and bad or oversized messages are ignored', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  const dc = FakePC.all[0].dc;
  dc.open();
  const chat = JSON.stringify({ type: 'chat', text: 'hi' });
  for (let i = 0; i < 40; i++) dc.onmessage({ data: chat });
  assert.equal(events.chats.filter((c) => !c.mine).length, 10); // burst of 10, clock frozen
  dc.onmessage({ data: JSON.stringify({ type: 'chat', text: 'x'.repeat(5000) }) });
  dc.onmessage({ data: 'garbage' });
  dc.onmessage({ data: new ArrayBuffer(8) });
  assert.equal(events.chats.filter((c) => !c.mine).length, 10);
});

test('muting a player silences their chat but not other messages', async () => {
  FakeWS.all[0].push(welcome(LOW));
  await tick();
  const dc = FakePC.all[0].dc;
  dc.open();
  lobby.mute(LOW, true);
  dc.onmessage({ data: JSON.stringify({ type: 'chat', text: 'hi' }) });
  assert.equal(events.chats.filter((c) => !c.mine).length, 0);
  dc.onmessage({ data: JSON.stringify({ type: 'decline' }) });
  assert.deepEqual(events.peerMessages.at(-1), [LOW, { type: 'decline' }]);
  assert.equal(lastRoster().find((p) => p.id === LOW).muted, true);
});

test('outgoing chat is rate limited and empty chat is refused', async () => {
  FakeWS.all[0].push(welcome(LOW));
  await tick();
  FakePC.all[0].dc.open();
  assert.equal(lobby.chat('   '), 'empty');
  assert.equal(lobby.chat('one'), 'ok');
  assert.equal(lobby.chat('two'), 'ok');
  assert.equal(lobby.chat('three'), 'rate'); // burst of 2, clock frozen
});

test('left removes the player and the peer, and tells the handler', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  ws.push({ type: 'left', id: HIGH });
  assert.equal(lastRoster().some((p) => p.id === HIGH), false);
  assert.deepEqual(events.left, [HIGH]);
});

test('status updates change the roster', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.push({ type: 'status', id: LOW, status: 'in-match' });
  assert.equal(lastRoster().find((p) => p.id === LOW).status, 'in-match');
});

test('sendTo reaches an open peer only', async () => {
  FakeWS.all[0].push(welcome(LOW, HIGH));
  await tick();
  FakePC.all[0].dc.open();
  assert.equal(lobby.sendTo(LOW, { type: 'decline' }), true);
  assert.equal(lobby.sendTo(HIGH, { type: 'decline' }), false);
  assert.equal(lobby.sendTo('nobody', { type: 'decline' }), false);
});

test('an incoming offer builds an answering peer that replies with an answer', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.push({ type: 'signal', from: LOW, data: { kind: 'offer', sdp: 'remote-offer' } });
  await tick();
  const answer = ws.sent.find((m) => m.type === 'signal' && m.data.kind === 'answer');
  assert.deepEqual(answer, { type: 'signal', to: LOW, data: { kind: 'answer', sdp: 'a' } });
  assert.equal(FakePC.all.at(-1).remoteDescription.sdp, 'remote-offer');
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/test/lobby.test.js`
Expected: FAIL with "Cannot find module '../lobby.js'".

- [ ] **Step 3: Write `web/lobby.js`**

```js
// Browser side of the lobby: a presence socket to the signalling service, and one WebRTC data
// channel to every other player (a full mesh, at most 16 players). Chat and challenges travel
// over those channels, never through the server, which only learns who is online and relays the
// handshakes between players.

import {
  cleanChat,
  createRateLimiter,
  displayName,
  parsePeerMessage,
  randomId,
  shouldOffer,
} from './lobby-protocol.js';

const STUN = [{ urls: 'stun:stun.cloudflare.com:3478' }];
const PEER_TIMEOUT_MS = 20_000;
const PEER_RETRY_MS = 2000;
const RECONNECT_MS = [1000, 2000, 4000, 8000, 15000];
const SOCKET_OPEN = 1;

export class Lobby {
  /**
   * @param {string} signalingUrl base URL of the signalling service
   * @param {string} nick already checked with cleanNick
   * @param {{onState?: (s: {phase: string}) => void, onRoster?: (players: object[]) => void,
   *          onChat?: (c: {from: {id: string, name: string}, text: string, mine: boolean}) => void,
   *          onPeerMessage?: (id: string, msg: object) => void,
   *          onPeerLeft?: (id: string) => void}} handlers
   * @param {object} [deps] browser APIs, overridable for tests
   */
  constructor(signalingUrl, nick, handlers, deps = {}) {
    this.base = signalingUrl.replace(/\/+$/, '');
    this.nick = nick;
    this.h = handlers;
    this.WS = deps.WebSocket ?? WebSocket;
    this.PC = deps.RTCPeerConnection ?? RTCPeerConnection;
    this.setTimer = deps.setTimeout ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = deps.clearTimeout ?? ((t) => clearTimeout(t));
    this.now = deps.now ?? (() => performance.now());
    this.newId = deps.randomId ?? randomId;
    this.myId = null;
    this.myStatus = 'lobby';
    this.ws = null;
    this.players = new Map(); // id -> {id, nick, status, link}
    this.peers = new Map(); // id -> {pc, dc, pendingIce, timer, retried, limiter}
    this.muted = new Set();
    this.closed = false;
    this.noRetry = false;
    this.attempt = 0;
    this.reconnectTimer = null;
    this.sendLimiter = createRateLimiter({ perSecond: 2, burst: 2, now: this.now });
  }

  /** Open the presence socket. Every (re)connection uses a fresh id, so a stale socket can
   *  never clash with the new one and nobody can take over someone else's id. */
  connect() {
    this.closed = false;
    this.myId = this.newId();
    this.players.clear();
    const ws = (this.ws = new this.WS(`${this.base.replace(/^http/, 'ws')}/lobby/ws`));
    ws.onopen = () => ws.send(JSON.stringify({ type: 'hello', id: this.myId, nick: this.nick }));
    ws.onmessage = (e) => this.#onServer(e.data);
    ws.onclose = () => this.#onSocketClosed(ws);
    ws.onerror = () => {}; // a close always follows
    this.h.onState?.({ phase: this.attempt ? 'reconnecting' : 'connecting' });
  }

  /** Send chat to everyone connected. Returns 'ok', 'empty' or 'rate'. */
  chat(text) {
    const clean = cleanChat(text);
    if (clean === null) return 'empty';
    if (!this.sendLimiter.take()) return 'rate';
    const payload = JSON.stringify({ type: 'chat', text: clean });
    for (const peer of this.peers.values()) {
      if (peer.dc.readyState === 'open') peer.dc.send(payload);
    }
    this.h.onChat?.({ from: { id: this.myId, name: displayName(this.nick, this.myId) }, text: clean, mine: true });
    return 'ok';
  }

  /** Send a message to one player over their data channel; false if not connected. */
  sendTo(id, msg) {
    const peer = this.peers.get(id);
    if (!peer || peer.dc.readyState !== 'open') return false;
    peer.dc.send(JSON.stringify(msg));
    return true;
  }

  setStatus(status) {
    this.myStatus = status;
    if (this.ws?.readyState === SOCKET_OPEN) this.ws.send(JSON.stringify({ type: 'status', status }));
    this.#emitRoster();
  }

  mute(id, on) {
    if (on) this.muted.add(id);
    else this.muted.delete(id);
    this.#emitRoster();
  }

  /** Try the presence socket again now instead of waiting for the backoff. */
  retryNow() {
    if (this.closed || this.noRetry) return;
    this.clearTimer(this.reconnectTimer);
    this.reconnectTimer = null;
    if (!this.ws || this.ws.readyState !== SOCKET_OPEN) this.connect();
  }

  close() {
    this.closed = true;
    this.clearTimer(this.reconnectTimer);
    for (const peer of this.peers.values()) this.#closePeer(peer);
    this.peers.clear();
    this.players.clear();
    try { this.ws?.close(); } catch { /* already closed */ }
  }

  // ---- presence socket ---------------------------------------------------------------

  #onServer(raw) {
    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    switch (msg?.type) {
      case 'welcome':
        this.attempt = 0;
        for (const p of Array.isArray(msg.roster) ? msg.roster : []) {
          if (p.id !== this.myId) this.#addPlayer(p);
        }
        this.h.onState?.({ phase: 'online' });
        this.#emitRoster();
        break;
      case 'joined':
        this.#addPlayer(msg.peer);
        this.#emitRoster();
        break;
      case 'left':
        this.#removePlayer(msg.id);
        this.#emitRoster();
        break;
      case 'status': {
        const p = this.players.get(msg.id);
        if (p) {
          p.status = msg.status === 'in-match' ? 'in-match' : 'lobby';
          this.#emitRoster();
        }
        break;
      }
      case 'signal':
        this.#onSignal(msg.from, msg.data).catch((err) => console.warn('[ntba-lobby] signal failed', err));
        break;
      case 'full':
        this.noRetry = true;
        this.h.onState?.({ phase: 'full' });
        break;
      case 'error':
        console.warn('[ntba-lobby] server error', msg.reason);
        break;
    }
  }

  #onSocketClosed(ws) {
    if (ws !== this.ws) return; // a stale socket
    for (const peer of this.peers.values()) this.#closePeer(peer);
    this.peers.clear();
    this.players.clear();
    this.#emitRoster();
    if (this.closed || this.noRetry) return;
    this.h.onState?.({ phase: 'reconnecting' });
    const delay = RECONNECT_MS[Math.min(this.attempt, RECONNECT_MS.length - 1)];
    this.attempt += 1;
    this.reconnectTimer = this.setTimer(() => {
      if (!this.closed) this.connect();
    }, delay);
  }

  #signal(to, data) {
    if (this.ws?.readyState === SOCKET_OPEN) this.ws.send(JSON.stringify({ type: 'signal', to, data }));
  }

  // ---- players and the mesh ----------------------------------------------------------

  #addPlayer(p) {
    if (!p || typeof p.id !== 'string' || this.players.has(p.id)) return;
    this.players.set(p.id, {
      id: p.id,
      nick: String(p.nick),
      status: p.status === 'in-match' ? 'in-match' : 'lobby',
      link: 'connecting',
    });
    this.#startPeer(p.id, false);
  }

  #removePlayer(id) {
    this.players.delete(id);
    const peer = this.peers.get(id);
    if (peer) {
      this.clearTimer(peer.timer);
      this.#closePeer(peer);
      this.peers.delete(id);
    }
    this.h.onPeerLeft?.(id);
  }

  #startPeer(id, retried) {
    const pc = new this.PC({ iceServers: STUN });
    const dc = pc.createDataChannel('lobby', { negotiated: true, id: 0 });
    const peer = {
      pc,
      dc,
      pendingIce: [],
      timer: null,
      retried,
      limiter: createRateLimiter({ perSecond: 5, burst: 10, now: this.now }),
    };
    this.peers.set(id, peer);
    dc.onopen = () => {
      this.clearTimer(peer.timer);
      this.#setLink(id, 'open');
    };
    dc.onclose = () => this.#fail(id, peer);
    dc.onmessage = (e) => this.#onPeerMessage(id, e.data);
    pc.onicecandidate = (e) => {
      if (e.candidate) this.#signal(id, { kind: 'ice', candidate: e.candidate.toJSON() });
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'failed') this.#fail(id, peer);
    };
    peer.timer = this.setTimer(() => this.#fail(id, peer), PEER_TIMEOUT_MS);
    if (shouldOffer(this.myId, id)) {
      (async () => {
        await pc.setLocalDescription(await pc.createOffer());
        this.#signal(id, { kind: 'offer', sdp: pc.localDescription.sdp });
      })().catch(() => this.#fail(id, peer));
    }
    return peer;
  }

  #fail(id, peer) {
    if (this.peers.get(id) !== peer) return;
    this.clearTimer(peer.timer);
    this.#closePeer(peer);
    this.peers.delete(id);
    this.#setLink(id, 'unreachable');
    // Only the side that makes the offers retries, and only once.
    if (!peer.retried && shouldOffer(this.myId, id)) {
      this.setTimer(() => {
        if (this.closed || !this.players.has(id) || this.peers.has(id)) return;
        this.#setLink(id, 'connecting');
        this.#startPeer(id, true);
      }, PEER_RETRY_MS);
    }
  }

  #closePeer(peer) {
    try {
      peer.dc.onclose = null;
      peer.dc.close();
      peer.pc.close();
    } catch {
      // Already closed.
    }
  }

  async #onSignal(from, data) {
    if (typeof from !== 'string' || !this.players.has(from) || data === null || typeof data !== 'object') return;
    if (data.kind === 'offer') {
      if (shouldOffer(this.myId, from)) return; // we make the offers for this pair; ignore a stray one
      const old = this.peers.get(from);
      if (old) {
        this.clearTimer(old.timer);
        this.#closePeer(old);
        this.peers.delete(from);
      }
      this.#setLink(from, 'connecting');
      const peer = this.#startPeer(from, true);
      await peer.pc.setRemoteDescription({ type: 'offer', sdp: data.sdp });
      await peer.pc.setLocalDescription(await peer.pc.createAnswer());
      this.#signal(from, { kind: 'answer', sdp: peer.pc.localDescription.sdp });
      await this.#flushIce(peer);
      return;
    }
    const peer = this.peers.get(from);
    if (!peer) return;
    if (data.kind === 'answer') {
      await peer.pc.setRemoteDescription({ type: 'answer', sdp: data.sdp });
      await this.#flushIce(peer);
    } else if (data.kind === 'ice') {
      if (peer.pc.remoteDescription) await peer.pc.addIceCandidate(data.candidate);
      else peer.pendingIce.push(data.candidate); // can outrun the offer or answer
    }
  }

  async #flushIce(peer) {
    for (const c of peer.pendingIce.splice(0)) await peer.pc.addIceCandidate(c);
  }

  #onPeerMessage(id, raw) {
    const player = this.players.get(id);
    const peer = this.peers.get(id);
    if (!player || !peer) return;
    if (!peer.limiter.take()) return; // flooding: drop
    const msg = parsePeerMessage(raw);
    if (msg === null) return;
    if (msg.type === 'chat') {
      if (this.muted.has(id)) return;
      this.h.onChat?.({ from: { id, name: displayName(player.nick, id) }, text: msg.text, mine: false });
    } else {
      this.h.onPeerMessage?.(id, msg);
    }
  }

  #setLink(id, link) {
    const p = this.players.get(id);
    if (p && p.link !== link) {
      p.link = link;
      this.#emitRoster();
    }
  }

  #emitRoster() {
    const me = {
      id: this.myId,
      nick: this.nick,
      name: displayName(this.nick, this.myId ?? 'xxx'),
      status: this.myStatus,
      link: 'self',
      muted: false,
    };
    const others = [...this.players.values()]
      .map((p) => ({ ...p, name: displayName(p.nick, p.id), muted: this.muted.has(p.id) }))
      .sort((a, b) => a.name.localeCompare(b.name));
    this.h.onRoster?.([me, ...others]);
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test web/test/`
Expected: PASS. If the "unreachable ... retried once" test fails on the timer indexes, print `timers` and fix the test's selection, not the 20 s and 2 s constants (the spec names retry once).

- [ ] **Step 5: Commit**

```bash
git add web/lobby.js web/test/lobby.test.js
git commit -m "Web: lobby presence socket and peer-to-peer data-channel mesh

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 8: Lobby UI, chat view and wiring

**Files:**
- Create: `web/chat.js`, `web/lobby-ui.js`
- Modify: `web/index.html`, `web/style.css`, `web/main.js`

**Interfaces:**
- Consumes: `Lobby` (Task 7), `cleanNick` (Task 6), `go`/`renderScreen`/`screen`/`showBusy`/`showMenu`/`endOnline`/`startOnline` in `main.js` (Task 2).
- Produces: `class ChatView { constructor(logEl); add({name, text, mine, system}) }` in `web/chat.js`; `renderRoster(listEl, players, actions)` in `web/lobby-ui.js`, where `actions = { onMute(id, on), onChallenge?(id), busy?() }`.

- [ ] **Step 1: Write `web/chat.js`**

```js
// Chat lines. Everything goes in with textContent, never innerHTML, and each message is its own
// block with dir="auto" so Hebrew and English each lay out in their own direction.

const MAX_LINES = 200;

export class ChatView {
  constructor(logEl) {
    this.log = logEl;
  }

  /** @param {{name?: string, text: string, mine?: boolean, system?: boolean}} line */
  add({ name = '', text, mine = false, system = false }) {
    const li = document.createElement('li');
    li.className = system ? 'chat-system' : mine ? 'chat-mine' : 'chat-theirs';
    if (!system) {
      const who = document.createElement('span');
      who.className = 'chat-name';
      who.dir = 'auto';
      who.textContent = name;
      li.append(who);
    }
    const body = document.createElement('span');
    body.className = 'chat-text';
    body.dir = 'auto';
    body.textContent = text;
    li.append(body);
    this.log.append(li);
    while (this.log.children.length > MAX_LINES) this.log.firstChild.remove();
    this.log.scrollTop = this.log.scrollHeight;
  }
}
```

- [ ] **Step 2: Write `web/lobby-ui.js`**

```js
// The player list. Names come from other players, so they are only ever set with textContent.

const LINK_TEXT = {
  self: 'you',
  open: 'connected',
  connecting: 'connecting…',
  unreachable: "can't reach for chat or challenges",
};

/**
 * @param {HTMLElement} list a <ul>
 * @param {{id: string, name: string, status: string, link: string, muted: boolean}[]} players
 * @param {{onMute?: (id: string, on: boolean) => void}} actions
 */
export function renderRoster(list, players, actions = {}) {
  const items = players.map((p) => {
    const li = document.createElement('li');
    li.className = `player player-${p.link}`;
    const name = document.createElement('span');
    name.className = 'player-name';
    name.dir = 'auto';
    name.textContent = p.name;
    const info = document.createElement('span');
    info.className = 'player-info';
    info.textContent = [p.status === 'in-match' ? 'in a match' : '', LINK_TEXT[p.link] ?? ''].filter(Boolean).join(' · ');
    li.append(name, info);
    if (p.link !== 'self') {
      const mute = document.createElement('button');
      mute.type = 'button';
      mute.textContent = p.muted ? 'Unmute' : 'Mute';
      mute.addEventListener('click', () => actions.onMute?.(p.id, !p.muted));
      li.append(mute);
    }
    return li;
  });
  list.replaceChildren(...items);
}
```

- [ ] **Step 3: Add the lobby markup**

In `web/index.html`, directly before `<section class="online" id="online" hidden aria-label="Online play">` add:

```html
    <section class="lobby" id="lobby" hidden aria-label="Online lobby">
      <form id="nick-form" autocomplete="off">
        <label>Your name
          <input id="nick-input" maxlength="40" size="14" dir="auto" placeholder="Dana / דנה" required>
        </label>
        <button type="submit">Enter lobby</button>
      </form>
      <div id="lobby-main" hidden>
        <p id="lobby-state" role="status"></p>
        <div class="lobby-grid">
          <ul id="roster" aria-label="Players online"></ul>
          <div class="chat">
            <ol id="chat-log" aria-live="polite"></ol>
            <form id="chat-form" autocomplete="off">
              <input id="chat-input" maxlength="600" dir="auto" placeholder="Say something… / כתבו משהו…" aria-label="Chat message">
              <button type="submit">Send</button>
            </form>
          </div>
        </div>
        <p class="lobby-note">Other players in the lobby can see your IP address - that is how peer-to-peer works. Chat is not stored.</p>
      </div>
      <p class="lobby-or">Or play with a room code:</p>
    </section>
```

(`maxlength="40"` on the inputs is deliberately looser than the 20/500-character limits: `cleanNick`/`cleanChat` are the real check, and they count characters, not UTF-16 units.)

- [ ] **Step 4: Add the styles**

Append to `web/style.css` (before the `@media` block):

```css
/* Lobby */
.lobby { width: 100%; max-width: 60em; display: flex; flex-direction: column; gap: 10px; }
.lobby[hidden], .lobby [hidden] { display: none; }
.lobby input, .lobby button {
  font: inherit; color: #fff; background: var(--panel); border: 1px solid var(--line); border-radius: 3px; padding: 4px 8px;
}
.lobby button { cursor: pointer; padding: 4px 12px; }
.lobby button:hover, .lobby button:focus-visible, .lobby input:focus-visible { border-color: var(--gold); outline: none; }
.lobby button:disabled { opacity: .45; cursor: default; border-color: var(--line); }
#nick-form { display: flex; gap: 8px; align-items: center; justify-content: center; flex-wrap: wrap; }
#lobby-state { margin: 0; min-height: 1.4em; color: var(--gold); text-align: center; }
.lobby-grid { display: grid; grid-template-columns: minmax(180px, 1fr) minmax(0, 2fr); gap: 12px; }
#roster, #chat-log { list-style: none; margin: 0; padding: 8px; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; }
#roster { display: flex; flex-direction: column; gap: 6px; max-height: 260px; overflow-y: auto; }
.player { display: flex; flex-wrap: wrap; gap: 4px 8px; align-items: center; }
.player-name { color: #fff; font-weight: bold; unicode-bidi: plaintext; }
.player-info { color: var(--muted); font-size: 12px; flex: 1 1 100%; }
.player-unreachable .player-name { color: var(--muted); }
.player button { padding: 0 8px; font-size: 12px; }
.chat { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
#chat-log { height: 220px; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; }
#chat-log li { display: block; }
.chat-name { color: var(--gold); margin-inline-end: .5em; unicode-bidi: plaintext; }
.chat-mine .chat-name { color: var(--orange); }
.chat-text { unicode-bidi: plaintext; overflow-wrap: anywhere; }
.chat-system { color: var(--muted); font-style: italic; }
#chat-form { display: flex; gap: 8px; }
#chat-input { flex: 1; min-width: 0; unicode-bidi: plaintext; }
.lobby-note, .lobby-or { margin: 0; text-align: center; color: var(--muted); font-size: 12px; }
```

and inside the existing `@media (max-width: 760px)` block add:

```css
  .lobby-grid { grid-template-columns: 1fr; }
```

- [ ] **Step 5: Wire `web/main.js`**

(a) Add imports after `import { nextScreen } from './flow.js';`:

```js
import { ChatView } from './chat.js';
import { Lobby } from './lobby.js';
import { renderRoster } from './lobby-ui.js';
import { cleanNick } from './lobby-protocol.js';
```

(b) After the `back: document.getElementById('menu-back'),` line in the `ui` object add:

```js
  lobby: document.getElementById('lobby'),
  nickForm: document.getElementById('nick-form'),
  nickInput: document.getElementById('nick-input'),
  lobbyMain: document.getElementById('lobby-main'),
  lobbyState: document.getElementById('lobby-state'),
  roster: document.getElementById('roster'),
  chatLog: document.getElementById('chat-log'),
  chatForm: document.getElementById('chat-form'),
  chatInput: document.getElementById('chat-input'),
```

(c) Replace `renderScreen` with:

```js
  function renderScreen() {
    ui.title.hidden = screen !== 'title';
    ui.titleOnline.hidden = !canOnline;
    ui.back.hidden = screen !== 'local' && screen !== 'online';
    ui.lobby.hidden = !(canOnline && screen === 'online');
    ui.panel.hidden = !(canOnline && (screen === 'online' || screen === 'match'));
    status.textContent = screen === 'title' ? 'Choose how to play' : '';
  }
```

(d) Insert this block directly before the `// ---- screens` comment:

```js
  // ---- lobby -------------------------------------------------------------------------
  const NICK_KEY = 'ntba.nick';
  const LOBBY_TEXT = {
    connecting: 'Connecting to the lobby…',
    online: 'In the lobby - chat is peer to peer.',
    reconnecting: "Lost the lobby connection - retrying… (the room code below still works)",
    full: 'The lobby is full (16 players). Try again soon, or play with a room code below.',
  };
  const chat = new ChatView(ui.chatLog);
  let lobby = null;

  function savedNick() {
    try { return localStorage.getItem(NICK_KEY) ?? ''; } catch { return ''; }
  }
  function saveNick(nick) {
    try { localStorage.setItem(NICK_KEY, nick); } catch { /* private mode: not remembered */ }
  }

  function enterLobby(nick) {
    ui.nickForm.hidden = true;
    ui.lobbyMain.hidden = false;
    lobby = new Lobby(SIGNALING_URL, nick, {
      onState: ({ phase }) => { ui.lobbyState.textContent = LOBBY_TEXT[phase] ?? ''; },
      onRoster: (players) => renderRoster(ui.roster, players, { onMute: (id, on) => lobby?.mute(id, on) }),
      onChat: (m) => chat.add({ name: m.from.name, text: m.text, mine: m.mine }),
    });
    lobby.connect();
  }

  function leaveLobby() {
    lobby?.close();
    lobby = null;
    ui.roster.replaceChildren();
    ui.chatLog.replaceChildren();
    ui.lobbyMain.hidden = true;
    ui.nickForm.hidden = false;
  }

  if (canOnline) {
    ui.nickInput.value = savedNick();
    ui.nickForm.addEventListener('submit', (e) => {
      e.preventDefault();
      const nick = cleanNick(ui.nickInput.value);
      if (nick === null) {
        say('Pick a name of 1 to 20 characters (letters, digits, Hebrew and emoji are fine).');
        return;
      }
      say('');
      saveNick(nick);
      enterLobby(nick);
    });
    ui.chatForm.addEventListener('submit', (e) => {
      e.preventDefault();
      const result = lobby?.chat(ui.chatInput.value);
      if (result === 'ok') ui.chatInput.value = '';
      else if (result === 'rate') chat.add({ text: 'Slow down a little - two messages a second at most.', system: true });
    });
  }

```

(e) In the `ui.back` click handler, change `go('menu');` to:

```js
    if (screen === 'online') leaveLobby();
    go('menu');
```

(f) In `startOnline`'s `onOpen`, after `go('match-start');` add `lobby?.setStatus('in-match');`, and at the end of `endOnline`, after `go('match-end');`, add `lobby?.setStatus('lobby');`.

- [ ] **Step 6: Verify in the browser** (needs Part B deployed; if it is not, you will see "Lost the lobby connection - retrying…", which is itself the check that the offline path degrades cleanly)

```bash
node --test web/test/
nix develop -c make serve
```
Open `http://localhost:8080/`, pick **Online**, then:
1. Enter the name `Dana`. The lobby shows "In the lobby" and your own row.
2. Open a second tab at the same address, enter `דנה`. Both tabs list both players; links go to "connected" within a few seconds.
3. Chat `hello` from tab 1 and `שלום` from tab 2. Check the Hebrew line is right-aligned and reads correctly, and a mixed line like `מה קורה? nothing much` renders with each part in its own direction.
4. Paste text containing an invisible right-to-left override (U+202E), for example `hi` + U+202E + `there`, into the chat box: it must arrive as plain `hithere` and not flip any text.
5. Click **Mute** for the other player and chat from them: nothing appears. **Unmute**.
6. Close one tab: the other's roster drops them within a few seconds.
7. **Main menu**: back to the title; Online again and the name field is pre-filled.
Stop the server.

- [ ] **Step 7: Commit and open the PR**

```bash
git add web
git commit -m "Web: online lobby with a player list and Hebrew/English peer-to-peer chat

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git push -u origin feat/lobby-chat
nix run nixpkgs#gh -- pr create --base main --title "Online lobby and peer-to-peer chat" --body "Part 3 of the lobby spec. Players in the lobby connect to each other directly (mesh, cap 16); chat never touches the server. Needs the presence service from the previous PR deployed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

---

# Part D: Challenges (PR 4)

### Task 9: Challenge reducer (pure)

**Files:**
- Create: `web/challenge.js`, `web/test/challenge.test.js`
- Modify: (none)

**Interfaces:**
- Consumes: peer message shapes from Task 6 (`challenge {room}`, `accept`, `decline`, `cancel`).
- Produces: `CHALLENGE_TIMEOUT_MS = 30_000`; `initialChallenges() -> {out: null, inc: null}`; `reduce(state, event, ctx) -> {state, effects}` where `ctx = {busy: boolean}`. Events: `{type:'start', to}`, `{type:'room', room}`, `{type:'msg', from, msg}`, `{type:'accept-in'}`, `{type:'decline-in'}`, `{type:'timeout', which:'out'|'inc'}`, `{type:'peer-left', id}`, `{type:'match-started'}`, `{type:'hosting-failed'}`. Effects (plain objects, any subset): `{host:true}`, `{cancelHost:true}`, `{send:{to, msg}}`, `{join: room}`, `{prompt:{from, room}}`, `{closePrompt:true}`, `{notify: key}`, `{timer:'out'|'inc'}`, `{clearTimer:'out'|'inc'}`. `notify` keys: `busy`, `accepted`, `declined`, `no-answer`, `simultaneous`, `hosting-failed`.

- [ ] **Step 0: Branch** (after PR #50 is merged)

```bash
cd /home/stags/Source/NtBA
git fetch origin && git log origin/main --oneline | grep -i "connect timer" || echo "PR #50 is not merged yet: stop and merge it first"
git switch -c feat/lobby-challenges main
```
(If Part C is not merged, branch from `feat/lobby-chat` instead.)

- [ ] **Step 1: Write the failing test**

Create `web/test/challenge.test.js`:

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { initialChallenges, reduce } from '../challenge.js';

const ROOM = 'K7M2QX';
const idle = { busy: false };
const run = (state, ev, ctx = idle) => reduce(state, ev, ctx);
const has = (effects, key) => effects.find((e) => key in e);

test('starting a challenge asks to host a room', () => {
  const r = run(initialChallenges(), { type: 'start', to: 'B' });
  assert.deepEqual(r.state.out, { to: 'B', phase: 'creating' });
  assert.ok(has(r.effects, 'host'));
});

test('you cannot start a challenge while busy or already challenging', () => {
  const busy = run(initialChallenges(), { type: 'start', to: 'B' }, { busy: true });
  assert.equal(busy.state.out, null);
  assert.deepEqual(has(busy.effects, 'notify'), { notify: 'busy' });
  const first = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  const second = run(first, { type: 'start', to: 'C' });
  assert.equal(second.state.out.to, 'B');
});

test('when the room is ready the challenge is sent and a timer starts', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  const r = run(s, { type: 'room', room: ROOM });
  assert.deepEqual(r.state.out, { to: 'B', phase: 'sent', room: ROOM });
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'B', msg: { type: 'challenge', room: ROOM } } });
  assert.deepEqual(has(r.effects, 'timer'), { timer: 'out' });
});

test('a room that arrives after the challenge was cancelled is closed', () => {
  const r = run(initialChallenges(), { type: 'room', room: ROOM });
  assert.ok(has(r.effects, 'cancelHost'));
  assert.equal(r.state.out, null);
});

test('accept moves the challenge on and stops the timer', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'msg', from: 'B', msg: { type: 'accept' } });
  assert.equal(r.state.out.phase, 'accepted');
  assert.deepEqual(has(r.effects, 'clearTimer'), { clearTimer: 'out' });
});

test('accept from someone else is ignored', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'msg', from: 'C', msg: { type: 'accept' } });
  assert.equal(r.state.out.phase, 'sent');
});

test('decline cancels hosting and tells the user', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'msg', from: 'B', msg: { type: 'decline' } });
  assert.equal(r.state.out, null);
  assert.ok(has(r.effects, 'cancelHost'));
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'declined' });
});

test('no answer within the timeout cancels and tells the target', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'timeout', which: 'out' });
  assert.equal(r.state.out, null);
  assert.ok(has(r.effects, 'cancelHost'));
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'B', msg: { type: 'cancel' } } });
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'no-answer' });
});

test('an incoming challenge shows a prompt and starts a timer', () => {
  const r = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } });
  assert.deepEqual(r.state.inc, { from: 'A', room: ROOM });
  assert.deepEqual(has(r.effects, 'prompt'), { prompt: { from: 'A', room: ROOM } });
  assert.deepEqual(has(r.effects, 'timer'), { timer: 'inc' });
});

test('an incoming challenge while busy is declined', () => {
  const r = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }, { busy: true });
  assert.equal(r.state.inc, null);
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'A', msg: { type: 'decline' } } });
});

test('a second incoming challenge while one is showing is declined', () => {
  let s = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  const r = run(s, { type: 'msg', from: 'C', msg: { type: 'challenge', room: 'AAAAAA' } });
  assert.equal(r.state.inc.from, 'A');
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'C', msg: { type: 'decline' } } });
});

test('accepting joins the room and tells the challenger', () => {
  let s = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  const r = run(s, { type: 'accept-in' });
  assert.equal(r.state.inc, null);
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'A', msg: { type: 'accept' } } });
  assert.deepEqual(has(r.effects, 'join'), { join: ROOM });
  assert.ok(has(r.effects, 'closePrompt'));
});

test('declining or timing out an incoming challenge declines it', () => {
  const s = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  for (const ev of [{ type: 'decline-in' }, { type: 'timeout', which: 'inc' }]) {
    const r = run(s, ev);
    assert.equal(r.state.inc, null);
    assert.deepEqual(has(r.effects, 'send'), { send: { to: 'A', msg: { type: 'decline' } } });
    assert.ok(has(r.effects, 'closePrompt'));
  }
});

test('the challenger cancelling removes the prompt', () => {
  const s = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  const r = run(s, { type: 'msg', from: 'A', msg: { type: 'cancel' } });
  assert.equal(r.state.inc, null);
  assert.ok(has(r.effects, 'closePrompt'));
  const other = run(s, { type: 'msg', from: 'Z', msg: { type: 'cancel' } });
  assert.equal(other.state.inc.from, 'A');
});

test('both players challenging each other at once ends cleanly with no stranded room', () => {
  // A has challenged B and is waiting; B's challenge to A arrives.
  let a = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  a = run(a, { type: 'room', room: ROOM }).state;
  const r = run(a, { type: 'msg', from: 'B', msg: { type: 'challenge', room: 'AAAAAA' } });
  assert.equal(r.state.out, null);
  assert.equal(r.state.inc, null);
  assert.ok(has(r.effects, 'cancelHost'));
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'simultaneous' });
  assert.ok(has(r.effects, 'clearTimer'));
  // The same race while A's room is still being created: the late room is closed.
  let early = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  early = run(early, { type: 'msg', from: 'B', msg: { type: 'challenge', room: 'AAAAAA' } }).state;
  assert.equal(early.out, null);
  const late = run(early, { type: 'room', room: ROOM });
  assert.ok(has(late.effects, 'cancelHost'));
});

test('the other player leaving clears challenges involving them', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'peer-left', id: 'B' });
  assert.equal(r.state.out, null);
  assert.ok(has(r.effects, 'cancelHost'));
  const inc = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  const r2 = run(inc, { type: 'peer-left', id: 'A' });
  assert.equal(r2.state.inc, null);
  assert.ok(has(r2.effects, 'closePrompt'));
  const unrelated = run(inc, { type: 'peer-left', id: 'Q' });
  assert.equal(unrelated.state.inc.from, 'A');
});

test('a match starting clears everything', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  s = run(s, { type: 'msg', from: 'B', msg: { type: 'accept' } }).state;
  const r = run(s, { type: 'match-started' });
  assert.deepEqual(r.state, { out: null, inc: null });
});

test('failing to host cancels the challenge', () => {
  const s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  const r = run(s, { type: 'hosting-failed' });
  assert.equal(r.state.out, null);
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'hosting-failed' });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/test/challenge.test.js`
Expected: FAIL with "Cannot find module '../challenge.js'".

- [ ] **Step 3: Write `web/challenge.js`**

```js
// Challenge state, as a pure reducer: (state, event, context) -> { state, effects }.
// `out` is the challenge we sent, `inc` the one we are being asked about. Effects are plain
// objects the page carries out (send a message, host a room, show a prompt, ...).

export const CHALLENGE_TIMEOUT_MS = 30_000;

export const initialChallenges = () => ({ out: null, inc: null });

const decline = (to) => ({ send: { to, msg: { type: 'decline' } } });

export function reduce(state, ev, ctx = { busy: false }) {
  const { out, inc } = state;
  switch (ev.type) {
    case 'start':
      if (out || inc || ctx.busy) return { state, effects: [{ notify: 'busy' }] };
      return { state: { out: { to: ev.to, phase: 'creating' }, inc }, effects: [{ host: true }] };

    case 'room':
      if (out?.phase !== 'creating') return { state, effects: [{ cancelHost: true }] }; // cancelled meanwhile
      return {
        state: { out: { ...out, phase: 'sent', room: ev.room }, inc },
        effects: [{ send: { to: out.to, msg: { type: 'challenge', room: ev.room } } }, { timer: 'out' }],
      };

    case 'msg':
      return onMessage(state, ev, ctx);

    case 'accept-in':
      if (!inc) return { state, effects: [] };
      return {
        state: { out, inc: null },
        effects: [{ send: { to: inc.from, msg: { type: 'accept' } } }, { join: inc.room }, { closePrompt: true }, { clearTimer: 'inc' }],
      };

    case 'decline-in':
      if (!inc) return { state, effects: [] };
      return { state: { out, inc: null }, effects: [decline(inc.from), { closePrompt: true }, { clearTimer: 'inc' }] };

    case 'timeout':
      if (ev.which === 'out' && out?.phase === 'sent') {
        return {
          state: { out: null, inc },
          effects: [{ send: { to: out.to, msg: { type: 'cancel' } } }, { cancelHost: true }, { notify: 'no-answer' }],
        };
      }
      if (ev.which === 'inc' && inc) {
        return { state: { out, inc: null }, effects: [decline(inc.from), { closePrompt: true }] };
      }
      return { state, effects: [] };

    case 'peer-left': {
      const effects = [];
      let next = state;
      if (out?.to === ev.id) {
        next = { ...next, out: null };
        effects.push({ cancelHost: true }, { clearTimer: 'out' });
      }
      if (inc?.from === ev.id) {
        next = { ...next, inc: null };
        effects.push({ closePrompt: true }, { clearTimer: 'inc' });
      }
      return { state: next, effects };
    }

    case 'match-started':
      return { state: initialChallenges(), effects: [{ clearTimer: 'out' }, { clearTimer: 'inc' }, { closePrompt: true }] };

    case 'hosting-failed':
      return { state: { out: null, inc }, effects: [{ notify: 'hosting-failed' }] };

    default:
      return { state, effects: [] };
  }
}

function onMessage(state, { from, msg }, ctx) {
  const { out, inc } = state;
  switch (msg.type) {
    case 'challenge':
      if (out?.to === from) {
        // Each of us challenged the other at once: cancel both and let them try again.
        return {
          state: { out: null, inc },
          effects: [{ cancelHost: true }, { clearTimer: 'out' }, decline(from), { notify: 'simultaneous' }],
        };
      }
      if (out || inc || ctx.busy) return { state, effects: [decline(from)] };
      return { state: { out, inc: { from, room: msg.room } }, effects: [{ prompt: { from, room: msg.room } }, { timer: 'inc' }] };

    case 'accept':
      if (out?.to !== from || out.phase !== 'sent') return { state, effects: [] };
      return { state: { out: { ...out, phase: 'accepted' }, inc }, effects: [{ clearTimer: 'out' }, { notify: 'accepted' }] };

    case 'decline':
      if (out?.to !== from) return { state, effects: [] };
      return { state: { out: null, inc }, effects: [{ cancelHost: true }, { clearTimer: 'out' }, { notify: 'declined' }] };

    case 'cancel':
      if (inc?.from !== from) return { state, effects: [] };
      return { state: { out, inc: null }, effects: [{ closePrompt: true }, { clearTimer: 'inc' }] };

    default:
      return { state, effects: [] };
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test web/test/`
Expected: PASS (all). Check by reading the "both players challenging" test that no path leaves `out` set without a `cancelHost`.

- [ ] **Step 5: Commit**

```bash
git add web/challenge.js web/test/challenge.test.js
git commit -m "Web: challenge state reducer

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 10: Challenge UI and wiring

**Files:**
- Modify: `web/lobby-ui.js`, `web/index.html`, `web/style.css`, `web/main.js`

**Interfaces:**
- Consumes: `reduce`, `initialChallenges`, `CHALLENGE_TIMEOUT_MS` (Task 9); `Lobby.sendTo`, `onPeerMessage`, `onPeerLeft` (Task 7); `renderRoster` (Task 8); `startOnline`, `join`, `showBusy`, `showMenu`, `say` in `main.js`.
- Produces: `startOnline(role, onRoom)` gains an optional callback receiving the room code; `renderRoster` actions gain `onChallenge(id)` and `busy()`.

- [ ] **Step 1: Challenge button in the roster**

In `web/lobby-ui.js`, inside `renderRoster`, directly after `li.append(mute);` add:

```js
      if (actions.onChallenge) {
        const challenge = document.createElement('button');
        challenge.type = 'button';
        challenge.textContent = 'Challenge';
        challenge.disabled = p.link !== 'open' || p.status !== 'lobby' || Boolean(actions.busy?.());
        challenge.addEventListener('click', () => actions.onChallenge(p.id));
        li.prepend(challenge); // first, so it is the obvious action
      }
```

Update the JSDoc `actions` type there to `{onMute?: ..., onChallenge?: (id: string) => void, busy?: () => boolean}`.

- [ ] **Step 2: Challenge prompt markup and style**

In `web/index.html`, inside `<div id="lobby-main" hidden>` just after the `lobby-state` paragraph add:

```html
        <div id="challenge-box" class="challenge-box" role="alertdialog" aria-labelledby="challenge-text" hidden>
          <p id="challenge-text"></p>
          <button type="button" id="challenge-accept">Accept</button>
          <button type="button" id="challenge-decline">Decline</button>
        </div>
```

Append to `web/style.css` (before the `@media` block):

```css
.challenge-box {
  display: flex; gap: 10px; align-items: center; justify-content: center; flex-wrap: wrap;
  padding: 8px 12px; border: 1px solid var(--gold); border-left: 4px solid var(--gold); border-radius: 4px; background: var(--panel);
}
.challenge-box p { margin: 0; color: #fff; unicode-bidi: plaintext; }
.lobby .challenge-box[hidden] { display: none; }
```

- [ ] **Step 3: Wire `web/main.js`**

(a) Add import after the `renderRoster` import:

```js
import { CHALLENGE_TIMEOUT_MS, initialChallenges, reduce } from './challenge.js';
```

(b) In the `ui` object add:

```js
  challengeBox: document.getElementById('challenge-box'),
  challengeText: document.getElementById('challenge-text'),
  challengeAccept: document.getElementById('challenge-accept'),
  challengeDecline: document.getElementById('challenge-decline'),
```

(c) Change the `startOnline` signature and its `onCode` handler:

```js
  function startOnline(role, onRoom) {
```

and replace the `onCode:` line with:

```js
      onCode: (code) => {
        say(`Room code: ${code} - share it with the other player.`);
        onRoom?.(code);
      },
```

(d) In the lobby block from Task 8, add this code before `function enterLobby`:

```js
  // ---- challenges (rules live in challenge.js) ---------------------------------------
  const CHALLENGE_TEXT = {
    busy: "You can't challenge right now - finish what you are doing first.",
    accepted: 'Challenge accepted - connecting…',
    declined: 'They declined the challenge.',
    'no-answer': 'No answer to your challenge.',
    simultaneous: 'You both challenged each other at once - try again.',
    'hosting-failed': "Couldn't set up the match - try again.",
  };
  let challenges = initialChallenges();
  const challengeTimers = { out: null, inc: null };
  let lastPlayers = [];
  const names = () => new Map(lastPlayers.map((p) => [p.id, p.name]));
  const isBusy = () => screen === 'match' || link !== null;

  function drawRoster() {
    renderRoster(ui.roster, lastPlayers, {
      onMute: (id, on) => lobby?.mute(id, on),
      onChallenge: (id) => dispatch({ type: 'start', to: id }),
      busy: isBusy,
    });
  }

  function dispatch(ev) {
    const result = reduce(challenges, ev, { busy: isBusy() });
    challenges = result.state;
    for (const fx of result.effects) runEffect(fx);
    drawRoster();
  }

  function runEffect(fx) {
    if (fx.send) lobby?.sendTo(fx.send.to, fx.send.msg);
    if (fx.host) hostForChallenge();
    if (fx.cancelHost && !online) {
      const l = link;
      link = null;
      l?.close();
      showMenu('');
    }
    if (fx.join) join(fx.join);
    if (fx.prompt) {
      ui.challengeText.textContent = `${names().get(fx.prompt.from) ?? 'Someone'} challenges you to a match.`;
      ui.challengeBox.hidden = false;
    }
    if (fx.closePrompt) ui.challengeBox.hidden = true;
    if (fx.notify) ui.lobbyState.textContent = CHALLENGE_TEXT[fx.notify] ?? '';
    if (fx.clearTimer) clearTimeout(challengeTimers[fx.clearTimer]);
    if (fx.timer) {
      clearTimeout(challengeTimers[fx.timer]);
      challengeTimers[fx.timer] = setTimeout(() => dispatch({ type: 'timeout', which: fx.timer }), CHALLENGE_TIMEOUT_MS);
    }
  }

  async function hostForChallenge() {
    showBusy('Challenging…');
    try {
      await startOnline('host', (room) => dispatch({ type: 'room', room }))?.host();
    } catch (err) {
      link?.close();
      link = null;
      showMenu(err instanceof ConnectError ? err.message : `Could not start an online game: ${err.message}`);
      dispatch({ type: 'hosting-failed' });
    }
  }

  ui.challengeAccept.addEventListener('click', () => dispatch({ type: 'accept-in' }));
  ui.challengeDecline.addEventListener('click', () => dispatch({ type: 'decline-in' }));

```

(e) In `enterLobby`'s handlers replace the `onRoster` line and add the two new handlers:

```js
      onRoster: (players) => {
        lastPlayers = players;
        drawRoster();
      },
      onPeerMessage: (from, msg) => dispatch({ type: 'msg', from, msg }),
      onPeerLeft: (id) => dispatch({ type: 'peer-left', id }),
```

(f) In `leaveLobby()` add, before `lobby?.close()`:

```js
    challenges = initialChallenges();
    ui.challengeBox.hidden = true;
    clearTimeout(challengeTimers.out);
    clearTimeout(challengeTimers.inc);
```

and after `ui.roster.replaceChildren();` add `lastPlayers = [];`.

(g) In `startOnline`'s `onOpen`, after `lobby?.setStatus('in-match');` add `dispatch({ type: 'match-started' });`, and at the end of `endOnline` after `lobby?.setStatus('lobby');` add `drawRoster();`.

`dispatch`, `drawRoster` and `isBusy` are used by functions defined earlier in `main()`; they are function declarations or are only called after setup, so order is safe. If the linter or runtime complains about `challenges`/`lastPlayers` used before definition, move the `let` lines for `challenges`, `challengeTimers` and `lastPlayers` up next to `let link = null;`.

- [ ] **Step 4: Run the unit tests**

Run: `node --test web/test/`
Expected: PASS.

- [ ] **Step 5: Verify live with two tabs** (needs Parts B and C deployed; PR #50 merged and deployed so the host waits for its guest)

```bash
nix develop -c make serve
```
Open `http://localhost:8080/` in two tabs, Online, enter two different names; wait for "connected".
1. In tab 1 click **Challenge** next to tab 2. Tab 2 shows "<name> challenges you to a match." with Accept/Decline.
2. Tab 2 **Decline**: tab 1 shows "They declined the challenge." and returns to the lobby; the room is gone.
3. Challenge again and wait 30 seconds without answering: tab 1 shows "No answer to your challenge." and tab 2's prompt disappears.
4. Challenge again and **Accept**: both tabs go into a match ("Online - you are Home / Away").
5. In a match, the other player's roster row says "in a match" and its Challenge button is disabled. **Leave online match**: both return to the lobby and the status goes back to lobby.
6. Click Challenge in both tabs within a second of each other: both show "You both challenged each other at once - try again." and neither is left hosting.
Stop the server.

- [ ] **Step 6: Commit and open the PR**

```bash
git add web
git commit -m "Web: challenge a player from the lobby into a match

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git push -u origin feat/lobby-challenges
nix run nixpkgs#gh -- pr create --base main --title "Lobby challenges" --body "Part 4 of the lobby spec: challenge a player from the roster; they accept or decline; on accept both run the existing room-code match handshake. Depends on the host connect timer fix (PR #50).

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

### Task 11: Docs and the final live check

**Files:**
- Modify: `wiki/Online-Play.md`, `wiki/Roadmap.md`

- [ ] **Step 1: Document the lobby**

In `wiki/Online-Play.md`, add a section before "Known limitations and follow-ups":

```markdown
## Lobby and chat

Choosing **Online** on the title screen opens a lobby. Pick a name (remembered in your browser,
shown to others as `name#ABC`), and you see everyone online. Chat works in Hebrew and English.

How it works: the signalling Worker keeps a roster of who is online (one global Durable Object,
`/lobby/ws`) and relays the WebRTC handshakes between players. Every player then opens a direct data
channel to every other player (up to 16), and chat and challenges travel over those channels. The
server never sees a chat message and nothing is stored. Because the connections are direct, other
players in the lobby can see your IP address.

To play someone, click **Challenge** next to their name. They have 30 seconds to accept; on accept
you both run the normal room-code handshake. Room codes still work from the same page.

Limits: 16 players, 500 characters per message, two messages a second, no chat history, and a
Mute button per player. A player behind a strict firewall or NAT may be listed but marked "can't
reach for chat or challenges".
```

and replace the line `* Players are not yet identified or matched beyond the room code.` in the limitations list with:

```markdown
* Names are self-chosen and not enforced: anyone can pick any name (a three-character id suffix
  tells same-named players apart).
```

In `wiki/Roadmap.md`, in the Online row, append " A lobby with Hebrew/English chat and challenges is in progress." (and change "in progress" to "done" when Part D merges).

- [ ] **Step 2: Publish and commit**

```bash
git add wiki
git commit -m "Wiki: document the lobby, chat and challenges

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
Publish the wiki the repo's way: push the commit to `main` (after the PRs merge), then `git push origin main:wiki-publish`, wait for the "Publish wiki (one-off)" workflow to succeed, and delete the branch with `git push origin --delete wiki-publish`.

- [ ] **Step 3: Final live check (after everything is merged and deployed)**

On https://eyalro.github.io/NtBA/ (not localhost), with three browsers or devices (for example two desktop browsers and a phone):
1. The title shows a "Development Version" badge on the game screen in every mode.
2. Local and Online both work; Online from the title shows the lobby.
3. All three join the lobby and see each other; chat in Hebrew and English across all three.
4. Challenge across devices, accept, play one minute, leave, return to the lobby.
5. Kill the network on one device briefly: its lobby shows "Lost the lobby connection - retrying…" and recovers; Local play is unaffected.
6. A 17th connection is not practical to try by hand; the server test covers it.

Report which device pairs could and could not connect peer-to-peer; that is the data for the TURN decision.

---

## Self-review notes

- **Spec coverage:** menu (Task 2), badge (Task 1), presence protocol and cap (Tasks 3-5), mesh and chat with Hebrew/RTL, limits, mute, IP notice, no history (Tasks 6-8), challenges with 30 s expiry and cancellation (Tasks 9-10), failure handling (reconnect and "can't reach" in Task 7, offline degrade checked in Task 8 Step 6), docs (Task 11). The "guest joins a dead room" message is the spec's separate follow-up and is not in this plan.
- **Names used across tasks:** `cleanNick`, `cleanChat`, `displayName`, `shouldOffer`, `parsePeerMessage`, `createRateLimiter`, `randomId` (Task 6); `Lobby` methods `connect/chat/sendTo/setStatus/mute/retryNow/close` and handlers `onState/onRoster/onChat/onPeerMessage/onPeerLeft` (Task 7); `renderRoster(list, players, {onMute, onChallenge, busy})` (Tasks 8, 10); reducer events/effects (Task 9) match their use in Task 10.
- **Known judgement calls:** the lobby panel sits above the existing room-code panel on the Online screen rather than replacing it; both input `maxlength` attributes are looser than the real limits because the code counts characters, not UTF-16 units.
