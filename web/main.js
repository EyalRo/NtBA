// Thin host for the NtBA wasm core: keyboard in, fixed-timestep ticks, pixels out. In online
// mode the same loop drives a lock-step session instead (see net.js and crates/ntba-net).

import { SIGNALING_URL } from './config.js';
import { ConnectError, PeerLink } from './net.js';

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
};

let held = 0;
// Presses shorter than one tick (a quick tap between frames) must still register.
let pending = 0;

window.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement) return; // typing a room code
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

  wasm.ntba_init((Date.now() & 0xffffffff) >>> 0);
  status.textContent = '';
  canvas.focus();

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
    wasm.ntba_net_discard();
    // Give the Bye a moment to leave before tearing the connection down.
    setTimeout(() => l?.close(), 250);
    showMenu(message);
    status.textContent = '';
  }

  function startOnline(role) {
    if (!SIGNALING_URL) return;
    ui.host.disabled = true;
    ui.form.querySelector('button').disabled = true;
    link = new PeerLink(SIGNALING_URL, {
      onCode: (code) => say(`Room code: ${code} - share it with the other player.`),
      onStatus: say,
      onOpen: () => {
        const seed = role === 'host' ? crypto.getRandomValues(new Uint32Array(1))[0] : 0;
        wasm.ntba_net_start(role === 'host' ? 0 : 1, seed, INPUT_DELAY_TICKS);
        wasm.ntba_net_set_open(1);
        online = true;
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
    return link;
  }

  async function host() {
    showBusy('Creating a room…');
    try {
      await startOnline('host')?.host();
    } catch (err) {
      const message = err instanceof ConnectError ? err.message : `Could not start an online game: ${err.message}`;
      link?.close();
      link = null;
      showMenu(message);
    }
  }

  async function join(code) {
    showBusy('Joining…');
    try {
      await startOnline('guest')?.join(code);
    } catch (err) {
      const message = err instanceof ConnectError ? err.message : `Could not join: ${err.message}`;
      link?.close();
      link = null;
      showMenu(message);
    }
  }

  if (SIGNALING_URL && typeof RTCPeerConnection !== 'undefined') {
    ui.panel.hidden = false;
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
