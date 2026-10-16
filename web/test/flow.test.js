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
