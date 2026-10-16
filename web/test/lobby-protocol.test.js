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
  assert.equal(cleanChat('hi\u202Ethere'), 'hithere');
  assert.equal(cleanChat('a\u0000b\u0007c'), 'abc');
  assert.equal(cleanChat('   '), null);
  assert.equal(cleanChat('\u202E\u202E'), null);
  assert.equal(cleanChat(42), null);
});

test('cleanChat limits length by characters', () => {
  assert.equal(cleanChat('x'.repeat(MAX_CHAT_CHARS)), 'x'.repeat(MAX_CHAT_CHARS));
  assert.equal(cleanChat('x'.repeat(MAX_CHAT_CHARS + 1)), null);
  assert.equal(cleanChat('🏀'.repeat(MAX_CHAT_CHARS)), '🏀'.repeat(MAX_CHAT_CHARS));
});

test('the client and server agree on which nicknames are acceptable', () => {
  const samples = [
    'Dana', '  דנה ', '', ' ', 'x'.repeat(20), 'x'.repeat(21), '🏀🏀', 'a\u0000', 'a\u202Eb', 'a\u2067b', 'a  b', '\t',
    ' a\n ', 'a\tb', 'a\u007fb', '\u202Eabc',
  ];
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
