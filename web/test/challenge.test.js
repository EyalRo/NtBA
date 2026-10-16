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

test('accept moves the challenge on and restarts the timer', () => {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  const r = run(s, { type: 'msg', from: 'B', msg: { type: 'accept' } });
  assert.equal(r.state.out.phase, 'accepted');
  assert.deepEqual(has(r.effects, 'timer'), { timer: 'out' });
  assert.equal(has(r.effects, 'clearTimer'), undefined);
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

function accepted() {
  let s = run(initialChallenges(), { type: 'start', to: 'B' }).state;
  s = run(s, { type: 'room', room: ROOM }).state;
  return run(s, { type: 'msg', from: 'B', msg: { type: 'accept' } }).state;
}

test('an accepted challenge that never connects times out and cancels', () => {
  const r = run(accepted(), { type: 'timeout', which: 'out' });
  assert.equal(r.state.out, null);
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'B', msg: { type: 'cancel' } } });
  assert.ok(has(r.effects, 'cancelHost'));
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'no-connect' });
});

test('the target leaving the presence socket after accepting does not cancel the room', () => {
  const s = accepted();
  const r = run(s, { type: 'peer-left', id: 'B' });
  assert.deepEqual(r.state, s);
  assert.deepEqual(r.effects, []);
});

test('abandoning an outgoing challenge cancels hosting in any phase', () => {
  const r = run(accepted(), { type: 'abandon' });
  assert.equal(r.state.out, null);
  assert.ok(has(r.effects, 'cancelHost'));
  assert.deepEqual(has(r.effects, 'clearTimer'), { clearTimer: 'out' });
  assert.deepEqual(run(initialChallenges(), { type: 'abandon' }).effects, []);
});

test('a match starting after accept clears the state and both timers', () => {
  const r = run(accepted(), { type: 'match-started' });
  assert.deepEqual(r.state, { out: null, inc: null });
  assert.deepEqual(r.effects.filter((e) => e.clearTimer).map((e) => e.clearTimer).sort(), ['inc', 'out']);
});

test('accepting while busy declines instead of joining', () => {
  const s = run(initialChallenges(), { type: 'msg', from: 'A', msg: { type: 'challenge', room: ROOM } }).state;
  const r = run(s, { type: 'accept-in' }, { busy: true });
  assert.equal(r.state.inc, null);
  assert.equal(has(r.effects, 'join'), undefined);
  assert.deepEqual(has(r.effects, 'send'), { send: { to: 'A', msg: { type: 'decline' } } });
  assert.ok(has(r.effects, 'closePrompt'));
  assert.deepEqual(has(r.effects, 'notify'), { notify: 'busy' });
  const ok = run(s, { type: 'accept-in' }, { busy: false });
  assert.deepEqual(has(ok.effects, 'join'), { join: ROOM });
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
