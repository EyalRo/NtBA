#!/usr/bin/env node
// Synthesises the C64 (SID chip) style sound effects into web/audio as 16-bit mono WAV.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'web', 'audio');
const RATE = 44100;
const OVERSAMPLE = 4;
const SR = RATE * OVERSAMPLE;
const CLOCK = 985248;
const FPS = 50;
const SPF = SR / FPS;
const ATTACK_MS = [2, 8, 16, 24, 38, 56, 68, 80, 100, 250, 500, 800, 1000, 3000, 5000, 8000];
const WAVE_BITS = { tri: 1, saw: 2, pulse: 4, noise: 8 };

const SEMITONE = { C: -9, D: -7, E: -5, F: -4, G: -2, A: 0, B: 2 };
function hz(note) {
  const [, name, acc, oct] = /^([A-G])(#|b)?(\d)$/.exec(note);
  const s = SEMITONE[name] + (acc === '#' ? 1 : acc === 'b' ? -1 : 0) + (Number(oct) - 4) * 12;
  return 440 * 2 ** (s / 12);
}

const val = (x, f) => (typeof x === 'function' ? x(f) : x);
const on = (...ranges) => (f) => ranges.some(([a, b]) => f >= a && f < b);
const pick = (list) => (f) => list[Math.min(f, list.length - 1)];
const glide = (from, to, frames, start = 0) => (f) =>
  from * (to / from) ** Math.min(Math.max((f - start) / frames, 0), 1);
const arp = (notes) => (f) => hz(notes[f % notes.length]);
const vibrato = (freq, depth, period, from) => (f) =>
  val(freq, f) * (f >= from ? 1 + depth * Math.sin((2 * Math.PI * (f - from)) / period) : 1);

/** Note steps [[note | null, frames], ...] as per-frame freq and gate; `gap` frames of gate-off retrigger each note. */
function tune(steps, { start = 0, gap = 1 } = {}) {
  const table = [];
  let last = hz(steps.find(([n]) => n)[0]);
  for (const [note, len] of steps) {
    if (note) last = hz(note);
    for (let i = 0; i < len; i++) table.push([last, note !== null && i < len - gap]);
  }
  const at = (f) => table[Math.min(Math.max(f - start, 0), table.length - 1)];
  return { freq: (f) => at(f)[0], gate: (f) => f >= start && f - start < table.length && at(f)[1], length: table.length };
}

function adsrPeriod(i) {
  return ((ATTACK_MS[i] / 1000) * SR) / 255;
}

function expPeriod(env) {
  if (env >= 93) return 1;
  if (env >= 54) return 2;
  if (env >= 26) return 4;
  if (env >= 14) return 8;
  if (env >= 6) return 16;
  return 30;
}

function renderSid(sound) {
  const n = sound.frames * SPF;
  const out = new Float64Array(n);
  const voices = sound.voices.map((v) => ({
    wave: 'tri', pw: 0x800, adsr: [0, 0, 15, 0], filter: false, ring: false, ...v,
    acc: 0, lfsr: 0x7ffff8, noise: 0, env: 0, stage: 'release', gateWas: false,
  }));
  const filter = { cutoff: 1000, res: 0, mode: 'lp', ...sound.filter };
  let ic1 = 0;
  let ic2 = 0;
  for (let f = 0; f < sound.frames; f++) {
    for (const v of voices) {
      const reg = Math.min(65535, Math.round((val(v.freq, f) * 2 ** 24) / CLOCK));
      v.inc = (reg * CLOCK) / SR;
      v.pwReg = Math.max(0, Math.min(4095, Math.round(val(v.pw, f))));
      v.bits = String(val(v.wave, f)).split('+').reduce((b, w) => b | (WAVE_BITS[w] ?? 0), 0);
      const gate = Boolean(val(v.gate, f));
      if (gate && !v.gateWas) v.stage = 'attack';
      if (!gate && v.gateWas) v.stage = 'release';
      v.gateWas = gate;
      const [a, d, s, r] = val(v.adsr, f);
      v.attackStep = 1 / adsrPeriod(a);
      v.decayStep = 1 / adsrPeriod(d);
      v.releaseStep = 1 / adsrPeriod(r);
      v.sustain = s * 17;
    }
    const fc = Math.min(12000, Math.max(30, val(filter.cutoff, f)));
    const g = Math.tan((Math.PI * fc) / SR);
    const k = 1 / (0.707 + val(filter.res, f) * 0.2);
    const a1 = 1 / (1 + g * (g + k));
    const a2 = g * a1;
    const a3 = g * a2;
    const mode = val(filter.mode, f);
    const volume = val(sound.volume ?? 15, f) / 15;

    for (let i = 0; i < SPF; i++) {
      for (const v of voices) {
        const before = Math.floor(v.acc);
        v.acc = (v.acc + v.inc) % 0x1000000;
        const now = Math.floor(v.acc);
        if (!(before & 0x80000) && now & 0x80000) {
          const r = v.lfsr;
          v.lfsr = ((r << 1) | (((r >>> 22) ^ (r >>> 17)) & 1)) & 0x7fffff;
          const q = v.lfsr;
          v.noise = (((q >>> 22) & 1) << 11) | (((q >>> 20) & 1) << 10) | (((q >>> 16) & 1) << 9)
            | (((q >>> 13) & 1) << 8) | (((q >>> 11) & 1) << 7) | (((q >>> 7) & 1) << 6)
            | (((q >>> 4) & 1) << 5) | (((q >>> 2) & 1) << 4);
        }
      }
      let direct = 0;
      let filtered = 0;
      voices.forEach((v, idx) => {
        const a = Math.floor(v.acc);
        let w = 0xfff;
        if (v.bits === 0) w = 0x800;
        if (v.bits & 1) {
          const src = voices[(idx + voices.length - 1) % voices.length];
          const msb = (a ^ (v.ring ? Math.floor(src.acc) : 0)) & 0x800000;
          w &= ((msb ? ~a : a) >>> 11) & 0xfff;
        }
        if (v.bits & 2) w &= a >>> 12;
        if (v.bits & 4) w &= (a >>> 12) >= v.pwReg ? 0xfff : 0;
        if (v.bits & 8) w &= v.noise;

        if (v.stage === 'attack') {
          v.env += v.attackStep;
          if (v.env >= 255) {
            v.env = 255;
            v.stage = 'decay';
          }
        } else if (v.stage === 'decay') {
          if (v.env > v.sustain) v.env = Math.max(v.sustain, v.env - v.decayStep / expPeriod(v.env));
        } else if (v.env > 0) {
          v.env = Math.max(0, v.env - v.releaseStep / expPeriod(v.env));
        }
        const s = ((w - 0x800) * v.env) / 255;
        if (v.filter) filtered += s;
        else direct += s;
      });
      const v3 = filtered - ic2;
      const v1 = a1 * ic1 + a2 * v3;
      const v2 = ic2 + a2 * ic1 + a3 * v3;
      ic1 = 2 * v1 - ic1;
      ic2 = 2 * v2 - ic2;
      let y = 0;
      if (mode.includes('lp')) y += v2;
      if (mode.includes('bp')) y += v1;
      if (mode.includes('hp')) y += filtered - k * v1 - v2;
      out[f * SPF + i] = (direct + y) * volume;
    }
  }
  return out;
}

function downsample(x) {
  const taps = 127;
  const fc = 18000 / SR;
  const h = new Float64Array(taps);
  const mid = (taps - 1) / 2;
  let sum = 0;
  for (let i = 0; i < taps; i++) {
    const t = i - mid;
    const sinc = t === 0 ? 2 * fc : Math.sin(2 * Math.PI * fc * t) / (Math.PI * t);
    const w = 0.42 - 0.5 * Math.cos((2 * Math.PI * i) / (taps - 1)) + 0.08 * Math.cos((4 * Math.PI * i) / (taps - 1));
    h[i] = sinc * w;
    sum += h[i];
  }
  const y = new Float64Array(Math.floor(x.length / OVERSAMPLE));
  for (let j = 0; j < y.length; j++) {
    let acc = 0;
    const c = j * OVERSAMPLE;
    for (let i = 0; i < taps; i++) {
      const idx = c + i - mid;
      if (idx >= 0 && idx < x.length) acc += x[idx] * h[i];
    }
    y[j] = acc / sum;
  }
  return y;
}

function finish(x, level) {
  let prevIn = 0;
  let prevOut = 0;
  for (let i = 0; i < x.length; i++) {
    const y = x[i] - prevIn + 0.9985 * prevOut;
    prevIn = x[i];
    prevOut = y;
    x[i] = y;
  }
  let peak = 0;
  for (const s of x) peak = Math.max(peak, Math.abs(s));
  const scale = peak ? 10 ** (level / 20) / peak : 0;
  let end = x.length;
  while (end > 0 && Math.abs(x[end - 1]) < peak * 1e-3) end--;
  end = Math.min(x.length, end + Math.round(RATE * 0.01));
  const fade = Math.round(RATE * 0.005);
  const y = new Float64Array(end);
  for (let i = 0; i < end; i++) y[i] = x[i] * scale * Math.min(1, (end - i) / fade);
  return y;
}

function wav(samples) {
  const buf = Buffer.alloc(44 + samples.length * 2);
  buf.write('RIFF', 0);
  buf.writeUInt32LE(36 + samples.length * 2, 4);
  buf.write('WAVEfmt ', 8);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20);
  buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(RATE, 24);
  buf.writeUInt32LE(RATE * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write('data', 36);
  buf.writeUInt32LE(samples.length * 2, 40);
  samples.forEach((s, i) => buf.writeInt16LE(Math.round(Math.max(-1, Math.min(1, s)) * 32767), 44 + i * 2));
  return buf;
}

const trill = (lo, hi) => (f) => (f % 2 ? hi : lo);

function whistle(blasts, frames) {
  const gate = on(...blasts);
  const pitch = (f) => (blasts.some(([a]) => f === a) ? 2500 : trill(2900, 3300)(f));
  return {
    frames,
    level: -3,
    volume: (f) => (f % 2 ? 15 : 11),
    filter: { cutoff: 3200, res: 10, mode: 'bp' },
    voices: [
      { wave: 'tri', freq: pitch, gate, adsr: [0, 0, 15, 1] },
      { wave: 'tri', freq: (f) => pitch(f) * 1.006, gate, adsr: [0, 0, 11, 1] },
      { wave: 'noise', freq: 3800, gate, adsr: [0, 0, 7, 1], filter: true },
    ],
  };
}

function buzzer(low, gateFrames, frames, level) {
  const gate = on([0, gateFrames]);
  return {
    frames,
    level,
    filter: { cutoff: 2200, res: 5, mode: 'lp' },
    voices: [
      { wave: 'saw', freq: low, gate, adsr: [1, 0, 15, 5], filter: true },
      { wave: 'pulse', pw: 0x500, freq: low * 1.058, gate, adsr: [1, 0, 15, 5], filter: true },
      { wave: 'saw', freq: low * 2.03, gate, adsr: [1, 0, 12, 5], filter: true },
    ],
  };
}

function dribble(start, noiseFreq) {
  return {
    frames: 10,
    level: -9,
    filter: { cutoff: 700, res: 2, mode: 'lp' },
    voices: [
      { wave: 'pulse', pw: 0x300, freq: pick([start, start * 0.62, start * 0.42, start * 0.34]), gate: on([0, 1]), adsr: [0, 4, 0, 4], filter: true },
      { wave: 'noise', freq: noiseFreq, gate: on([0, 1]), adsr: [0, 1, 0, 1] },
    ],
  };
}

function blip(steps, level) {
  const t = tune(steps, { gap: 0 });
  return {
    frames: t.length + 6,
    level,
    voices: [{ wave: 'pulse', pw: 0x800, freq: t.freq, gate: t.gate, adsr: [0, 0, 15, 3] }],
  };
}

const scoreLead = tune([['C5', 3], ['E5', 3], ['G5', 3], ['C6', 14]], { gap: 0 });
const fanfare = tune([['G4', 4], ['C5', 4], ['E5', 4], ['G5', 6], ['E5', 3], ['G5', 28]]);
const fanfareBass = tune([['C3', 6], ['C3', 6], ['G2', 6], ['C3', 30]]);
const winLead = tune([
  ['C5', 4], ['E5', 4], ['G5', 4], ['C6', 8],
  ['G5', 4], ['C6', 4], ['E6', 8],
  ['D6', 4], ['E6', 4], ['G6', 26],
]);
const winChord = (f) => arp(f < 20 ? ['C5', 'E5', 'G5'] : f < 36 ? ['F5', 'A5', 'C6'] : f < 44 ? ['G5', 'B5', 'D6'] : ['C6', 'E6', 'G6'])(f);
const winBass = tune([
  ['C3', 4], ['C3', 4], ['C3', 4], ['C3', 4], ['C3', 4],
  ['F2', 4], ['F2', 4], ['F2', 4], ['F2', 4],
  ['G2', 4], ['G2', 4],
  ['C3', 26],
]);
const loseLead = tune([['G4', 15], ['F#4', 15], ['F4', 15], ['E4', 45]], { gap: 2 });
const wah = (f) => {
  const t = f < 45 ? f % 15 : f - 45;
  return t < 6 ? 400 + t * 300 : Math.max(500, 2200 - (t - 6) * 90);
};
const crowdSwell = (f) => 11 + Math.round(2 * Math.sin(f * 0.21) + 2 * Math.sin(f * 0.077 + 1));

const SOUNDS = {
  whistle_start: whistle([[0, 8], [14, 48]], 54),
  whistle: whistle([[0, 16]], 20),
  buzzer_final: buzzer(110, 85, 100, -2),
  buzzer_shotclock: buzzer(146.8, 40, 52, -4),
  score_2: {
    frames: 50,
    level: -3,
    voices: [
      { wave: 'pulse', pw: (f) => 0x400 + f * 90, freq: scoreLead.freq, gate: on([0, 23]), adsr: [0, 8, 9, 8] },
      { wave: 'tri', freq: arp(['C6', 'E6', 'G6']), gate: on([9, 24]), adsr: [0, 9, 0, 9] },
      { wave: 'tri', freq: pick([hz('C4'), hz('C4'), hz('C4'), hz('G3'), hz('G3'), hz('G3'), hz('E3'), hz('E3'), hz('E3'), hz('C3')]), gate: on([0, 23]), adsr: [0, 8, 6, 6] },
    ],
  },
  score_3: {
    frames: 80,
    level: -2,
    filter: { cutoff: glide(400, 2200, 20), res: 6, mode: 'lp' },
    voices: [
      { wave: 'pulse', pw: (f) => 0x600 + 0x300 * Math.sin(f / 5), freq: vibrato(fanfare.freq, 0.012, 8, 27), gate: fanfare.gate, adsr: [0, 6, 12, 8] },
      { wave: 'pulse', pw: 0x600, freq: arp(['C5', 'E5', 'G5']), gate: on([21, 54]), adsr: [0, 10, 0, 10] },
      { wave: 'saw', freq: fanfareBass.freq, gate: fanfareBass.gate, adsr: [0, 8, 10, 8], filter: true },
    ],
  },
  swish: {
    frames: 25,
    level: -9,
    filter: { cutoff: glide(7000, 1500, 12), res: 8, mode: 'bp' },
    voices: [
      { wave: 'noise', freq: 3800, gate: on([0, 2]), adsr: [1, 7, 0, 7], filter: true },
      { wave: 'noise', freq: 2200, gate: on([2, 3]), adsr: [0, 6, 0, 6], filter: true },
    ],
  },
  shot: {
    frames: 25,
    level: -8,
    filter: { cutoff: 3000, res: 0, mode: 'hp' },
    voices: [
      { wave: 'tri', freq: glide(300, 1300, 10), gate: on([0, 2]), adsr: [0, 7, 0, 7] },
      { wave: 'pulse', pw: 0x300, freq: glide(150, 650, 10), gate: on([0, 2]), adsr: [0, 6, 0, 6] },
      { wave: 'noise', freq: 3000, gate: on([0, 1]), adsr: [0, 3, 0, 3], filter: true },
    ],
  },
  pass: {
    frames: 12,
    level: -10,
    filter: { cutoff: 2500, res: 6, mode: 'bp' },
    voices: [
      { wave: 'tri', freq: glide(800, 1600, 4), gate: on([0, 1]), adsr: [0, 4, 0, 4] },
      { wave: 'noise', freq: 3000, gate: on([0, 1]), adsr: [0, 2, 0, 2], filter: true },
    ],
  },
  catch: {
    frames: 10,
    level: -12,
    filter: { cutoff: 1200, res: 0, mode: 'lp' },
    voices: [
      { wave: 'tri', freq: pick([523, 784]), gate: on([0, 1]), adsr: [0, 3, 0, 3] },
      { wave: 'noise', freq: 1500, gate: on([0, 1]), adsr: [0, 1, 0, 1], filter: true },
    ],
  },
  rebound: blip([['C5', 2], ['G5', 3]], -16),
  dribble_1: dribble(180, 400),
  dribble_2: dribble(160, 340),
  steal: {
    frames: 24,
    level: -6,
    filter: { cutoff: glide(7000, 900, 6), res: 6, mode: 'lp' },
    voices: [
      { wave: 'saw', freq: pick([1800, 1300, 950, 700, 520, 400]), gate: on([0, 6]), adsr: [0, 0, 15, 2], filter: true },
      { wave: 'noise', freq: 3000, gate: on([0, 2]), adsr: [0, 3, 0, 3] },
      { wave: 'pulse', pw: 0x800, ...tune([['A5', 2], ['E6', 5]], { start: 7 }), adsr: [0, 5, 0, 5] },
    ],
  },
  block: {
    frames: 25,
    level: -5,
    filter: { cutoff: glide(4000, 250, 8), res: 4, mode: 'lp' },
    voices: [
      { wave: 'noise', freq: 1500, gate: on([0, 1]), adsr: [0, 7, 0, 7], filter: true },
      { wave: 'pulse', pw: 0x300, freq: glide(160, 55, 6), gate: on([0, 1]), adsr: [0, 6, 0, 6], filter: true },
      { wave: 'tri', freq: 1200, gate: on([0, 1]), adsr: [0, 2, 0, 2] },
    ],
  },
  miss: {
    frames: 55,
    level: -5,
    voices: [
      { wave: 'tri', ring: true, freq: 740, gate: on([0, 1], [9, 10]), adsr: [0, 9, 0, 9] },
      { wave: 'tri', ring: true, freq: 1310, gate: on([0, 1], [9, 10]), adsr: [0, 8, 0, 8] },
      { wave: 'noise', freq: 1093, gate: on([0, 1]), adsr: [0, 1, 0, 1] },
    ],
  },
  jump: {
    frames: 16,
    level: -14,
    voices: [{ wave: 'pulse', pw: (f) => 0x400 + f * 0x80, freq: glide(200, 800, 8), gate: on([0, 8]), adsr: [0, 0, 15, 3] }],
  },
  tick: {
    frames: 6,
    level: -20,
    voices: [{ wave: 'pulse', pw: 0x800, freq: 1760, gate: on([0, 3]), adsr: [0, 0, 15, 0] }],
  },
  crowd_cheer: {
    frames: 175,
    level: -3,
    volume: crowdSwell,
    filter: { cutoff: (f) => 1400 + 500 * Math.sin(f * 0.37) + 300 * Math.sin(f * 0.13), res: 3, mode: 'bp' },
    voices: [
      { wave: 'noise', freq: 1800, gate: on([0, 100]), adsr: [10, 0, 15, 10], filter: true },
      { wave: 'noise', freq: 900, gate: on([0, 100]), adsr: [11, 0, 13, 11], filter: true },
      { wave: 'noise', freq: 3400, gate: on([0, 100]), adsr: [9, 0, 10, 9], filter: true },
    ],
  },
  ui_move: {
    frames: 6,
    level: -14,
    voices: [{ wave: 'tri', freq: hz('C6'), gate: on([0, 1]), adsr: [0, 2, 0, 2] }],
  },
  ui_select: blip([['C6', 2], ['G6', 4]], -18),
  ui_back: blip([['G5', 2], ['C5', 4]], -18),
  ui_notify: {
    frames: 45,
    level: -9,
    voices: [
      { wave: 'tri', freq: hz('E6'), gate: on([0, 1]), adsr: [0, 8, 0, 8] },
      { wave: 'tri', freq: hz('G6'), gate: on([3, 4]), adsr: [0, 8, 0, 8] },
      { wave: 'tri', freq: hz('C7'), gate: on([6, 7]), adsr: [0, 9, 0, 9] },
    ],
  },
  jingle_win: {
    frames: 95,
    level: -3,
    filter: { cutoff: 900, res: 4, mode: 'lp' },
    voices: [
      { wave: 'pulse', pw: (f) => 0x500 + 0x300 * Math.sin(f / 6), freq: vibrato(winLead.freq, 0.012, 8, 50), gate: winLead.gate, adsr: [0, 6, 12, 8] },
      { wave: 'pulse', pw: 0x400, freq: winChord, gate: on([0, 68]), adsr: [0, 0, 7, 8] },
      { wave: 'saw', freq: winBass.freq, gate: winBass.gate, adsr: [0, 6, 8, 8], filter: true },
    ],
  },
  jingle_lose: {
    frames: 105,
    level: -4,
    filter: { cutoff: wah, res: 9, mode: 'lp' },
    voices: [
      { wave: 'saw', freq: vibrato(loseLead.freq, 0.03, 7, 50), gate: loseLead.gate, adsr: [2, 0, 15, 6], filter: true },
      { wave: 'saw', freq: (f) => vibrato(loseLead.freq, 0.03, 7, 50)(f) * 0.502, gate: loseLead.gate, adsr: [2, 0, 15, 6], filter: true },
    ],
  },
};

mkdirSync(OUT_DIR, { recursive: true });
const only = process.argv.slice(2);
for (const [name, sound] of Object.entries(SOUNDS)) {
  if (only.length && !only.includes(name)) continue;
  const samples = finish(downsample(renderSid(sound)), sound.level);
  writeFileSync(join(OUT_DIR, `${name}.wav`), wav(samples));
  console.log(`${name.padEnd(18)} ${(samples.length / RATE).toFixed(2)}s`);
}
