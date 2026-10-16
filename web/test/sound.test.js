import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { CUE_SOUNDS, CueTracker, FINAL_BUZZER, endingSound, loadPref, savePref } from '../sound.js';

const audio = new URL('../audio/', import.meta.url);

test('every cue the core raises has sounds, in the same order', () => {
  const src = readFileSync(new URL('../../crates/ntba-core/src/game.rs', import.meta.url), 'utf8');
  const body = /pub enum Cue \{([^}]*)\}/.exec(src)[1];
  const names = body.split(',').map((s) => s.trim()).filter(Boolean);
  assert.equal(CUE_SOUNDS.length, names.length);
  assert.equal(names[FINAL_BUZZER], 'FinalBuzzer');
  const labels = readFileSync(new URL('../sound.js', import.meta.url), 'utf8')
    .match(/\], \/\/ (\w+)/g).map((m) => m.split('// ')[1]);
  assert.deepEqual(labels, names);
});

test('every sound a cue plays exists', () => {
  for (const name of CUE_SOUNDS.flat()) {
    const files = name === 'dribble' ? ['dribble_1', 'dribble_2'] : [name];
    for (const f of files) assert.ok(existsSync(new URL(`${f}.wav`, audio)), f);
  }
});

test('the tracker reports cues that fired since the last frame', () => {
  const t = new CueTracker(3);
  assert.deepEqual(t.update(1, [1, 0, 0]), [0]);
  assert.deepEqual(t.update(2, [1, 0, 0]), []);
  assert.deepEqual(t.update(5, [1, 3, 1]), [1, 2]);
});

test('a new game (ticks went back) starts counting from zero', () => {
  const t = new CueTracker(2);
  t.update(900, [1, 40]);
  assert.deepEqual(t.update(0, [0, 0]), []);
  assert.deepEqual(t.update(1, [1, 0]), [0]);
});

test('the jingle after the buzzer follows the player side', () => {
  assert.equal(endingSound([20, 10], 0), 'jingle_win');
  assert.equal(endingSound([20, 10], 1), 'jingle_lose');
  assert.equal(endingSound([12, 12], 0), null);
  assert.equal(endingSound([3, 9], null), 'crowd_cheer');
});

test('sound is off unless the player turned it on', () => {
  const map = new Map();
  const storage = { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, v) };
  assert.equal(loadPref(storage), false);
  savePref(storage, true);
  assert.equal(loadPref(storage), true);
  savePref(storage, false);
  assert.equal(loadPref(storage), false);
  assert.equal(loadPref(null), false);
  assert.equal(loadPref({ getItem: () => { throw new Error('blocked'); } }), false);
});
