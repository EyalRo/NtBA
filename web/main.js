// Thin host for the NtBA wasm core: keyboard in, fixed-timestep ticks, pixels out. In online
// mode the same loop drives a lock-step session instead (see net.js and crates/ntba-net).

import { SIGNALING_URL } from './config.js';
import { ConnectError, PeerLink } from './net.js';
import { nextScreen } from './flow.js';
import { guardedAttempt } from './link-guard.js';
import { ChatView } from './chat.js';
import { Lobby } from './lobby.js';
import { renderRoster } from './lobby-ui.js';
import { CHALLENGE_TIMEOUT_MS, initialChallenges, reduce } from './challenge.js';
import { cleanNick } from './lobby-protocol.js';

const BUTTONS = {
  UP: 1 << 0, DOWN: 1 << 1, LEFT: 1 << 2, RIGHT: 1 << 3,
  PASS: 1 << 4, SHOOT: 1 << 5, SWITCH: 1 << 6, START: 1 << 7,
};

// Home pad in the low byte of the word passed to ntba_step, away pad in the next byte.
const P1 = 0;
const P2 = 8;

const KEYMAP = {
  KeyW: BUTTONS.UP << P1, KeyS: BUTTONS.DOWN << P1, KeyA: BUTTONS.LEFT << P1, KeyD: BUTTONS.RIGHT << P1,
  KeyZ: BUTTONS.PASS << P1, KeyX: BUTTONS.SHOOT << P1, KeyC: BUTTONS.SWITCH << P1,
  ArrowUp: BUTTONS.UP << P2, ArrowDown: BUTTONS.DOWN << P2, ArrowLeft: BUTTONS.LEFT << P2, ArrowRight: BUTTONS.RIGHT << P2,
  KeyJ: BUTTONS.PASS << P2, KeyK: BUTTONS.SHOOT << P2, KeyL: BUTTONS.SWITCH << P2,
  Enter: BUTTONS.START << P1,
};

// Why an online match ended (codes from ntba_net_end_reason).
const END_TEXT = {
  1: 'The other player left the match.',
  2: 'You left the online match.',
  3: 'The two games went out of sync, so the match was stopped. Please play again.',
  4: 'The other player is running a different version of the game. Reload both pages and try again.',
  5: 'The connection to the other player was lost.',
  6: 'The other player stopped responding.',
  7: 'The other player sent something unexpected, so the match was stopped.',
};

const INPUT_DELAY_TICKS = 6; // 100 ms; the host's choice, adopted by the guest
const CODE_RE = /^[A-HJKMNP-Z2-9]{6}$/;

const status = document.getElementById('status');
const canvas = document.getElementById('screen');
const ctx = canvas.getContext('2d');
const ui = {
  panel: document.getElementById('online'),
  menu: document.getElementById('online-menu'),
  host: document.getElementById('host-btn'),
  form: document.getElementById('join-form'),
  code: document.getElementById('join-code'),
  leave: document.getElementById('leave-btn'),
  text: document.getElementById('online-status'),
  title: document.getElementById('menu'),
  titleLocal: document.getElementById('menu-local'),
  titleOnline: document.getElementById('menu-online'),
  back: document.getElementById('menu-back'),
  lobby: document.getElementById('lobby'),
  nickForm: document.getElementById('nick-form'),
  nickInput: document.getElementById('nick-input'),
  lobbyMain: document.getElementById('lobby-main'),
  lobbyState: document.getElementById('lobby-state'),
  lobbyRetry: document.getElementById('lobby-retry'),
  roster: document.getElementById('roster'),
  chatLog: document.getElementById('chat-log'),
  chatForm: document.getElementById('chat-form'),
  chatInput: document.getElementById('chat-input'),
  challengeBox: document.getElementById('challenge-box'),
  challengeText: document.getElementById('challenge-text'),
  challengeAccept: document.getElementById('challenge-accept'),
  challengeDecline: document.getElementById('challenge-decline'),
};

let held = 0;
let screen = 'title'; // 'title' | 'local' | 'online' | 'match', see flow.js
// Presses shorter than one tick (a quick tap between frames) must still register.
let pending = 0;

window.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement) return; // typing a room code
  if (screen !== 'local' && screen !== 'match') return; // title and lobby take no game input
  const bit = KEYMAP[e.code];
  if (bit === undefined) return;
  e.preventDefault();
  held |= bit;
  pending |= bit;
});
window.addEventListener('keyup', (e) => {
  const bit = KEYMAP[e.code];
  if (bit === undefined) return;
  e.preventDefault();
  held &= ~bit;
});
window.addEventListener('blur', () => { held = 0; pending = 0; });

async function main() {
  const response = await fetch('ntba.wasm');
  if (!response.ok) throw new Error(`ntba.wasm: HTTP ${response.status} (run scripts/build-web.sh first)`);
  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  const wasm = instance.exports;

  const width = wasm.ntba_width();
  const height = wasm.ntba_height();
  const tickHz = wasm.ntba_tick_hz();
  const tickMs = 1000 / tickHz;
  canvas.width = width;
  canvas.height = height;
  const image = ctx.createImageData(width, height);

  const newSeed = () => (Date.now() & 0xffffffff) >>> 0;
  wasm.ntba_init(newSeed());

  // ---- online play -------------------------------------------------------------------
  const NONE = 0xffffffff;
  let link = null; // PeerLink while an online match is being set up or played
  let online = false; // true once the lock-step session is running
  const decoder = new TextDecoder();

  const say = (text) => { ui.text.textContent = text; };

  function showMenu(message = '') {
    ui.menu.hidden = false;
    ui.leave.hidden = true;
    ui.host.disabled = false;
    ui.form.querySelector('button').disabled = false;
    say(message);
  }

  function showBusy(message) {
    ui.menu.hidden = true;
    ui.leave.hidden = false;
    say(message);
  }

  function drainLogs() {
    for (let n; (n = wasm.ntba_log_next() >>> 0) !== NONE;) {
      const msg = decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.ntba_log_ptr(), n));
      const level = wasm.ntba_log_level();
      (level === 3 ? console.error : level === 2 ? console.warn : level === 1 ? console.info : console.debug)(`[ntba-net] ${msg}`);
    }
  }

  // Send everything the session queued; a closed link keeps messages queued (and dropped on discard).
  function drainNet() {
    if (link?.isOpen) {
      for (let n; (n = wasm.ntba_net_pop() >>> 0) !== NONE;) {
        const data = new Uint8Array(wasm.memory.buffer, wasm.ntba_net_out_ptr(), n).slice();
        if (wasm.ntba_net_out_channel() === 1) link.sendFast(data);
        else link.sendReliable(data);
      }
    }
    drainLogs();
  }

  function receive(data) {
    const ptr = wasm.ntba_net_alloc(data.length);
    if (!ptr) return;
    new Uint8Array(wasm.memory.buffer, ptr, data.length).set(data);
    wasm.ntba_net_push();
  }

  function endOnline(message) {
    const l = link;
    link = null;
    online = false;
    abandonChallenge(); // no-op after a match (match-started cleared it); else resets a failed challenge
    wasm.ntba_net_discard();
    // Give the Bye a moment to leave before tearing the connection down.
    setTimeout(() => l?.close(), 250);
    showMenu(message);
    status.textContent = '';
    go('match-end');
    lobby?.setStatus('lobby');
    drawRoster();
  }

  function startOnline(role, onRoom) {
    if (!SIGNALING_URL) return;
    ui.host.disabled = true;
    ui.form.querySelector('button').disabled = true;
    if (link) { // never leave a second link feeding the one session
      const old = link;
      link = null;
      old.close();
    }
    link = new PeerLink(SIGNALING_URL, {
      onCode: (code) => {
        say(`Room code: ${code} - share it with the other player.`);
        onRoom?.(code);
      },
      onStatus: say,
      onOpen: () => {
        const seed = role === 'host' ? crypto.getRandomValues(new Uint32Array(1))[0] : 0;
        wasm.ntba_net_start(role === 'host' ? 0 : 1, seed, INPUT_DELAY_TICKS);
        wasm.ntba_net_set_open(1);
        online = true;
        go('match-start');
        lobby?.setStatus('in-match');
        dispatch({ type: 'match-started' });
        held = 0;
        pending = 0;
        showBusy('Connected - starting…');
        canvas.focus();
      },
      onMessage: (data) => { if (online) receive(data); },
      onClose: (reason) => {
        if (online && wasm.ntba_net_status() !== 3) {
          wasm.ntba_net_set_open(0); // the session notices on its next frame
          return;
        }
        if (!online) endOnline(reason);
      },
    });
    drawRoster(); // challenge buttons and Accept follow the busy state
    return link;
  }

  function failOnline(prefix) {
    return (err) => {
      const message = err instanceof ConnectError ? err.message : `${prefix}: ${err.message}`;
      link?.close();
      link = null;
      showMenu(message);
      drawRoster();
    };
  }

  async function host() {
    showBusy('Creating a room…');
    await guardedAttempt(startOnline('host'), () => link, (l) => l?.host(), failOnline('Could not start an online game'));
  }

  async function join(code) {
    showBusy('Joining…');
    await guardedAttempt(startOnline('guest'), () => link, (l) => l?.join(code), failOnline('Could not join'));
  }

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
        abandonChallenge(); // before closing: it may need to cancel the pending challenge
        const l = link;
        link = null;
        l?.close();
        showMenu('');
        drawRoster();
      }
    });
    showMenu();
  }

  // ---- lobby -------------------------------------------------------------------------
  const NICK_KEY = 'ntba.nick';
  const LOBBY_TEXT = {
    connecting: 'Connecting to the lobby\u2026',
    online: 'In the lobby - chat is peer to peer.',
    unreachable: "Can't reach the lobby. Retrying\u2026 (the room code below still works)",
    reconnecting: "Lost the lobby connection - retrying\u2026 (the room code below still works)",
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

  // ---- challenges (rules live in challenge.js) ---------------------------------------
  const CHALLENGE_TEXT = {
    busy: "You can't challenge right now - finish what you are doing first.",
    accepted: 'Challenge accepted - connecting\u2026',
    declined: 'They declined the challenge.',
    'no-answer': 'No answer to your challenge.',
    simultaneous: 'You both challenged each other at once - try again.',
    'hosting-failed': "Couldn't set up the match - try again.",
    'no-connect': "They didn't connect - try again.",
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
    ui.challengeAccept.disabled = isBusy(); // the reducer also refuses a busy accept
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
      // The name sits in its own <bdi> so a Hebrew name cannot reorder the rest of the sentence.
      const who = document.createElement('bdi');
      who.textContent = names().get(fx.prompt.from) ?? 'Someone';
      ui.challengeText.replaceChildren(who, document.createTextNode(' challenges you to a match.'));
      ui.challengeBox.hidden = false;
    }
    if (fx.closePrompt) {
      ui.challengeBox.hidden = true;
      ui.challengeText.replaceChildren();
    }
    if (fx.notify) ui.lobbyState.textContent = CHALLENGE_TEXT[fx.notify] ?? '';
    if (fx.clearTimer) clearTimeout(challengeTimers[fx.clearTimer]);
    if (fx.timer) {
      clearTimeout(challengeTimers[fx.timer]);
      challengeTimers[fx.timer] = setTimeout(() => dispatch({ type: 'timeout', which: fx.timer }), CHALLENGE_TIMEOUT_MS);
    }
  }

  // Drop our outgoing challenge (Leave, or the link failed): tell the target if they saw it.
  function abandonChallenge() {
    const out = challenges.out;
    if (!out) return;
    if (out.phase === 'sent' || out.phase === 'accepted') lobby?.sendTo(out.to, { type: 'cancel' });
    dispatch({ type: 'abandon' });
  }

  async function hostForChallenge() {
    showBusy('Challenging\u2026');
    const l = startOnline('host', (room) => {
      if (link === l) dispatch({ type: 'room', room });
      else l?.close();
    });
    await guardedAttempt(l, () => link, (x) => x?.host(), (err) => {
      link?.close();
      link = null;
      showMenu(err instanceof ConnectError ? err.message : `Could not start an online game: ${err.message}`);
      dispatch({ type: 'hosting-failed' });
    });
  }

  ui.challengeAccept.addEventListener('click', () => dispatch({ type: 'accept-in' }));
  ui.lobbyRetry.addEventListener('click', () => lobby?.retryNow());
  ui.challengeDecline.addEventListener('click', () => dispatch({ type: 'decline-in' }));

  function enterLobby(nick) {
    if (lobby) return;
    ui.nickForm.hidden = true;
    ui.lobbyMain.hidden = false;
    lobby = new Lobby(SIGNALING_URL, nick, {
      onState: ({ phase, everConnected }) => {
        const key = phase === 'reconnecting' && !everConnected ? 'unreachable' : phase;
        ui.lobbyState.textContent = LOBBY_TEXT[key] ?? '';
        ui.lobbyRetry.hidden = phase !== 'reconnecting' && phase !== 'full';
      },
      onRoster: (players) => {
        lastPlayers = players;
        drawRoster();
      },
      onPeerMessage: (from, msg) => dispatch({ type: 'msg', from, msg }),
      onPeerLeft: (id) => dispatch({ type: 'peer-left', id }),
      onChat: (m) => chat.add({ name: m.from.name, text: m.text, mine: m.mine }),
    });
    lobby.connect();
  }

  function leaveLobby() {
    challenges = initialChallenges();
    ui.challengeBox.hidden = true;
    ui.lobbyRetry.hidden = true;
    clearTimeout(challengeTimers.out);
    clearTimeout(challengeTimers.inc);
    lobby?.close();
    lobby = null;
    ui.roster.replaceChildren();
    lastPlayers = [];
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

  // ---- screens -----------------------------------------------------------------------
  function renderScreen() {
    ui.title.hidden = screen !== 'title';
    ui.titleOnline.hidden = !canOnline;
    ui.back.hidden = screen !== 'local' && screen !== 'online';
    ui.lobby.hidden = !(canOnline && screen === 'online');
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
    if (screen === 'online') leaveLobby();
    go('menu');
    if (canOnline) showMenu('');
  });
  renderScreen();

  // ---- main loop ---------------------------------------------------------------------
  let last = performance.now();
  let acc = 0;
  let pingShown = 0;

  function runOnlineTicks() {
    // Both keymaps steer the one local player; the session decides which team that is.
    const word = held | pending;
    const pad = (word | (word >>> 8)) & 0xff;
    while (acc >= tickMs) {
      const result = wasm.ntba_net_step(pad);
      pending = 0;
      drainNet();
      if (result === 3) {
        const reason = wasm.ntba_net_end_reason();
        endOnline(END_TEXT[reason] ?? 'The online match ended.');
        return;
      }
      if (result !== 1) {
        // Connecting or waiting for the peer: don't pile up owed ticks into a later burst.
        acc = Math.min(acc, tickMs * 12);
        return;
      }
      acc -= tickMs;
    }
  }

  function frame(now) {
    // Clamp so a background tab doesn't trigger a huge catch-up burst.
    acc += Math.min(now - last, 250);
    last = now;
    if (online) {
      runOnlineTicks();
      if (online && now - pingShown > 1000) {
        pingShown = now;
        const rtt = wasm.ntba_net_rtt() >>> 0;
        const side = wasm.ntba_net_side() === 0 ? 'Home (gold)' : 'Away (green)';
        const ping = rtt === NONE ? '…' : `${Math.round((rtt * 1000) / tickHz)} ms`;
        const waiting = wasm.ntba_net_status() === 1 ? ' · connecting' : '';
        say(`Online - you are ${side} · ping ${ping}${waiting}`);
      }
    } else {
      while (acc >= tickMs) {
        wasm.ntba_step(held | pending);
        pending = 0;
        acc -= tickMs;
      }
    }
    wasm.ntba_render();
    // Memory may have moved; always re-create the view from the current buffer.
    const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.ntba_framebuffer(), width * height * 4);
    image.data.set(pixels);
    ctx.putImageData(image, 0, 0);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main().catch((err) => {
  console.error(err);
  status.textContent = `Failed to start: ${err.message}`;
});
