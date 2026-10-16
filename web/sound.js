// C64-style sound effects from web/audio. Off until the player turns them on.

// Indexed like ntba_core::game::Cue.
export const CUE_SOUNDS = [
  ['whistle_start'], // TipOff
  ['buzzer_final'], // FinalBuzzer
  ['buzzer_shotclock'], // ShotClock
  ['score_2', 'swish'], // Score2
  ['score_3', 'swish', 'crowd_cheer'], // Score3
  ['shot'], // Shot
  ['pass'], // Pass
  ['catch'], // Catch
  ['rebound'], // Rebound
  ['dribble'], // Dribble
  ['dribble'], // Bounce
  ['steal'], // Steal
  ['block'], // Block
  ['miss'], // Miss
  ['jump'], // Jump
  ['tick'], // Countdown
];
export const FINAL_BUZZER = 1;

const FILES = [
  'whistle_start', 'buzzer_final', 'buzzer_shotclock', 'score_2', 'score_3', 'swish', 'shot',
  'pass', 'catch', 'rebound', 'dribble_1', 'dribble_2', 'steal', 'block', 'miss', 'jump', 'tick',
  'crowd_cheer', 'jingle_win', 'jingle_lose', 'ui_move', 'ui_select', 'ui_back', 'ui_notify',
];
const PREF_KEY = 'ntba.sound';

/** Turns the game's ever-growing cue counters into the cues fired since the last call. */
export class CueTracker {
  constructor(count = CUE_SOUNDS.length) {
    this.last = new Array(count).fill(0);
    this.ticks = 0;
  }

  update(ticks, counts) {
    if (ticks < this.ticks) this.last.fill(0);
    this.ticks = ticks;
    const fired = [];
    counts.forEach((c, i) => {
      if (c > this.last[i]) fired.push(i);
      this.last[i] = c;
    });
    return fired;
  }
}

/** The jingle after the final buzzer for the player on `side`; null for a tie or no side. */
export function endingSound(scores, side) {
  if (side === null) return 'crowd_cheer';
  const [mine, theirs] = [scores[side], scores[1 - side]];
  if (mine === theirs) return null;
  return mine > theirs ? 'jingle_win' : 'jingle_lose';
}

export function loadPref(storage) {
  try { return storage?.getItem(PREF_KEY) === 'on'; } catch { return false; }
}

export function savePref(storage, on) {
  try { storage?.setItem(PREF_KEY, on ? 'on' : 'off'); } catch { /* private mode: not remembered */ }
}

export class Sound {
  constructor(base = 'audio/') {
    this.base = base;
    this.enabled = false;
    this.ctx = null;
    this.out = null;
    this.buffers = new Map();
    this.loading = null;
    this.dribbles = 0;
  }

  /** Call from a user gesture the first time. */
  async setEnabled(on) {
    this.enabled = on;
    if (!on) {
      await this.ctx?.suspend();
      return;
    }
    if (!this.ctx) {
      const Ctx = globalThis.AudioContext ?? globalThis.webkitAudioContext;
      if (!Ctx) return;
      this.ctx = new Ctx();
      this.out = this.ctx.createGain();
      this.out.gain.value = 0.8;
      this.out.connect(this.ctx.destination);
    }
    await this.ctx.resume();
    await this.load();
  }

  load() {
    this.loading ??= Promise.all(FILES.filter((name) => !this.buffers.has(name)).map(async (name) => {
      for (let attempt = 1; ; attempt++) {
        try {
          const res = await fetch(`${this.base}${name}.wav`);
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          this.buffers.set(name, await this.ctx.decodeAudioData(await res.arrayBuffer()));
          return;
        } catch (err) {
          if (attempt === 3) {
            console.warn(`sound ${name}: ${err.message}`);
            return;
          }
        }
      }
    })).then(() => {
      if (this.buffers.size < FILES.length) this.loading = null; // try the missing ones next time
    });
    return this.loading;
  }

  play(name, delay = 0) {
    if (!this.enabled || this.ctx?.state !== 'running') return;
    if (name === 'dribble') name = `dribble_${1 + (this.dribbles++ % 2)}`;
    const buffer = this.buffers.get(name);
    if (!buffer) return;
    const src = this.ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(this.out);
    src.start(this.ctx.currentTime + delay);
  }
}
