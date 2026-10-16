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
import { CUE_SOUNDS, CueTracker, FINAL_BUZZER, Sound, endingSound, loadPref, savePref } from './sound.js';

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

const status = document.getElementById('status');
const canvas = document.getElementById('screen');
const ctx = canvas.getContext('2d');
const ui = {
  panel: document.getElementById('online'),
  leave: document.getElementById('leave-btn'),
  text: document.getElementById('online-status'),
  title: document.getElementById('menu'),
  titleLocal: document.getElementById('menu-local'),
  titleCpu: document.getElementById('menu-cpu'),
  cpuSetup: document.getElementById('cpu-setup'),
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
  soundToggle: document.getElementById('sound-toggle'),
  colourblindToggle: document.getElementById('colourblind-toggle'),
};

const PLAYING = new Set(['local', 'cpu', 'match']);
const CPU_LEVELS = ['Novice', 'Pro', 'All-Star'];
const SIDES = ['Home (gold)', 'Away (green)'];

let held = 0;
let screen = 'title'; // 'title' | 'local' | 'cpu-setup' | 'cpu' | 'online' | 'match', see flow.js
// Presses shorter than one tick (a quick tap between frames) must still register.
let pending = 0;

// ---- sound -------------------------------------------------------------------------
const sound = new Sound();
const store = (() => { try { return localStorage; } catch { return null; } })();
let soundOn = loadPref(store);

function showSound() {
  ui.soundToggle.setAttribute('aria-pressed', String(soundOn));
  ui.soundToggle.textContent = soundOn ? 'Sound: on' : 'Sound: off';
}

function setSound(on) {
  soundOn = on;
  savePref(store, on);
  showSound();
  sound.setEnabled(on)
    .then(() => { if (on) sound.play('ui_select'); })
    .catch((err) => console.warn(`sound: ${err.message}`));
}

ui.soundToggle.addEventListener('click', () => {
  setSound(!soundOn);
  if (PLAYING.has(screen)) canvas.focus();
});
showSound();
if (soundOn) {
  // Start a remembered "on" on the first click or key.
  const wake = () => {
    window.removeEventListener('pointerdown', wake, true);
    window.removeEventListener('keydown', wake, true);
    if (soundOn) sound.setEnabled(true).catch((err) => console.warn(`sound: ${err.message}`));
  };
  window.addEventListener('pointerdown', wake, true);
  window.addEventListener('keydown', wake, true);
}

// ---- colour-blind mode: away team wears blue instead of green ---------------------------
const CB_KEY = 'ntba.colourblind';
let colourblindOn = (() => { try { return store?.getItem(CB_KEY) === 'on'; } catch { return false; } })();

let wasmExports = null; // set once the core is loaded; the saved colour-blind choice is applied then

function showColourblind() {
  ui.colourblindToggle.setAttribute('aria-pressed', String(colourblindOn));
  ui.colourblindToggle.textContent = colourblindOn ? 'Colour-blind mode: on' : 'Colour-blind mode: off';
  wasmExports?.ntba_colourblind(colourblindOn ? 1 : 0);
}

function setColourblind(on) {
  colourblindOn = on;
  try { store?.setItem(CB_KEY, on ? 'on' : 'off'); } catch { /* private mode: not remembered */ }
  showColourblind();
}

showColourblind();
ui.colourblindToggle.addEventListener('click', () => {
  setColourblind(!colourblindOn);
  if (PLAYING.has(screen)) canvas.focus();
});

const BACK_BUTTONS = new Set(['menu-back', 'leave-btn', 'challenge-decline']);
document.addEventListener('click', (e) => {
  const button = e.target instanceof Element ? e.target.closest('button') : null;
  if (!button || button === ui.soundToggle || button === ui.colourblindToggle) return;
  sound.play(BACK_BUTTONS.has(button.id) ? 'ui_back' : 'ui_select');
});
ui.cpuSetup.addEventListener('change', () => sound.play('ui_move'));

window.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement) return; // typing a room code
  if (e.code === 'KeyM' && !e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey) {
    setSound(!soundOn);
    return;
  }
  if (e.code === 'KeyB' && !e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey) {
    setColourblind(!colourblindOn);
    return;
  }
  if (!PLAYING.has(screen)) return; // title, setup and lobby take no game input
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
  wasmExports = wasm;
  showColourblind();

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
    ui.leave.hidden = true;
    say(message);
  }

  function showBusy(message) {
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

  async function join(code) {
    showBusy('Joining…');
    await guardedAttempt(startOnline('guest'), () => link, (l) => l?.join(code), failOnline('Could not join'));
  }

  const canOnline = Boolean(SIGNALING_URL) && typeof RTCPeerConnection !== 'undefined';
  if (canOnline) {
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
      sound.play('ui_notify');
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
      onChat: (m) => {
        chat.add({ name: m.from.name, text: m.text, mine: m.mine });
        if (!m.mine) sound.play('ui_notify');
      },
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
    ui.cpuSetup.hidden = screen !== 'cpu-setup';
    ui.back.hidden = !['local', 'cpu-setup', 'cpu', 'online'].includes(screen);
    ui.lobby.hidden = !(canOnline && screen === 'online');
    ui.panel.hidden = !(canOnline && (screen === 'online' || screen === 'match'));
    status.textContent = {
      title: 'Choose how to play',
      'cpu-setup': 'Pick a level and a side',
      cpu: `VS CPU \u00b7 ${CPU_LEVELS[cpu.level]} \u00b7 you are ${SIDES[cpu.side]}`,
    }[screen] ?? '';
  }

  function go(event) {
    screen = nextScreen(screen, event);
    renderScreen();
    if (PLAYING.has(screen)) canvas.focus();
  }

  ui.titleLocal.addEventListener('click', () => {
    wasm.ntba_init(newSeed()); // a fresh game; the idle demo behind the title is discarded
    held = 0;
    pending = 0;
    go('local');
  });
  ui.titleOnline.addEventListener('click', () => go('online'));

  // ---- vs cpu ------------------------------------------------------------------------
  const CPU_KEY = 'ntba.cpu';
  const cpu = { level: 0, side: 0 };
  try {
    const saved = JSON.parse(localStorage.getItem(CPU_KEY) ?? '{}');
    if (Number.isInteger(saved.level) && CPU_LEVELS[saved.level]) cpu.level = saved.level;
    if (Number.isInteger(saved.side) && SIDES[saved.side]) cpu.side = saved.side;
  } catch { /* private mode or a bad value: defaults */ }

  ui.titleCpu.addEventListener('click', () => {
    ui.cpuSetup.elements['cpu-level'].value = String(cpu.level);
    ui.cpuSetup.elements['cpu-side'].value = String(cpu.side);
    go('cpu');
  });
  ui.cpuSetup.addEventListener('submit', (e) => {
    e.preventDefault();
    cpu.level = Number(ui.cpuSetup.elements['cpu-level'].value);
    cpu.side = Number(ui.cpuSetup.elements['cpu-side'].value);
    try { localStorage.setItem(CPU_KEY, JSON.stringify(cpu)); } catch { /* not remembered */ }
    wasm.ntba_init(newSeed());
    wasm.ntba_cpu(1 - cpu.side, cpu.level, crypto.getRandomValues(new Uint32Array(1))[0]);
    held = 0;
    pending = 0;
    go('play');
  });

  // Back to the title from a game, the cpu setup or the lobby. A game is reset so the next one is fresh.
  function returnToTitle() {
    if (screen === 'local' || screen === 'cpu') {
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
  }
  ui.back.addEventListener('click', returnToTitle);

  // The final buzzer: no restart key. Hold the final score for a moment, then drop to the title,
  // where the player can start a new local game or rejoin the lobby.
  const FINAL_HOLD_MS = 5000;
  let finalSince = null;
  function finishGame() {
    if (screen === 'match') {
      wasm.ntba_net_stop(); // tells the peer; both sides reach the buzzer on the same tick
      drainNet();
      endOnline('');
    }
    returnToTitle();
  }
  renderScreen();

  // ---- game sounds -------------------------------------------------------------------
  const cues = new CueTracker();
  const cueCounts = CUE_SOUNDS.map(() => 0);
  function playCues() {
    for (let i = 0; i < cueCounts.length; i++) cueCounts[i] = wasm.ntba_cue(i) >>> 0;
    const fired = cues.update(wasm.ntba_ticks() >>> 0, cueCounts);
    if (!PLAYING.has(screen)) return; // the idle demo stays quiet
    for (const id of fired) for (const name of CUE_SOUNDS[id]) sound.play(name);
    if (fired.includes(FINAL_BUZZER)) {
      const side = screen === 'cpu' ? cpu.side : screen === 'match' ? wasm.ntba_net_side() : null;
      const name = endingSound([wasm.ntba_score(0), wasm.ntba_score(1)], side);
      if (name) sound.play(name, 1.6);
    }
  }

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
        const word = held | pending;
        // Against the computer both keymaps steer the player's side.
        wasm.ntba_step(screen === 'cpu' ? ((word | (word >>> 8)) & 0xff) << (8 * cpu.side) : word);
        pending = 0;
        acc -= tickMs;
      }
    }
    playCues();
    if (PLAYING.has(screen)) {
      if (!wasm.ntba_game_over()) finalSince = null;
      else if (finalSince === null) finalSince = now;
      else if (now - finalSince >= FINAL_HOLD_MS) {
        finalSince = null;
        finishGame();
      }
    } else {
      finalSince = null;
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
