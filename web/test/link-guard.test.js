import assert from 'node:assert/strict';
import { test } from 'node:test';
import { guardedAttempt } from '../link-guard.js';

const fakeLink = () => ({ closed: 0, close() { this.closed += 1; } });

test('reports success and does nothing else', async () => {
  const link = fakeLink();
  let failures = 0;
  const ok = await guardedAttempt(link, () => link, async () => {}, () => { failures += 1; });
  assert.equal(ok, true);
  assert.equal(failures, 0);
  assert.equal(link.closed, 0);
});

test('a failure on the current link goes to onFailure and leaves closing to the handler', async () => {
  const link = fakeLink();
  const seen = [];
  const ok = await guardedAttempt(link, () => link, async () => { throw new Error('no room'); }, (e) => seen.push(e.message));
  assert.equal(ok, false);
  assert.deepEqual(seen, ['no room']);
  assert.equal(link.closed, 0);
});

test('a failure after the link was replaced closes only the stale link and stays silent', async () => {
  const stale = fakeLink();
  const current = fakeLink();
  let failures = 0;
  const ok = await guardedAttempt(stale, () => current, async () => { throw new Error('late'); }, () => { failures += 1; });
  assert.equal(ok, false);
  assert.equal(failures, 0);
  assert.equal(stale.closed, 1);
  assert.equal(current.closed, 0);
});

test('a failure after the link was cleared also closes the stale link silently', async () => {
  const stale = fakeLink();
  let failures = 0;
  await guardedAttempt(stale, () => null, async () => { throw new Error('late'); }, () => { failures += 1; });
  assert.equal(failures, 0);
  assert.equal(stale.closed, 1);
});

test('a missing link (no signalling URL) is a harmless no-op', async () => {
  const ok = await guardedAttempt(null, () => null, async (l) => l?.host(), () => assert.fail('no failure expected'));
  assert.equal(ok, true);
});
