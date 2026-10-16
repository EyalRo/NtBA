import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MAX_LOBBY_MESSAGE_BYTES, cleanNick, parseClientMessage } from '../src/lobby-logic.js';

const ID = 'abcDEF123456';
const msg = (o) => JSON.stringify(o);

test('cleanNick trims, collapses spaces and accepts Hebrew and English', () => {
  assert.equal(cleanNick('  Dana  '), 'Dana');
  assert.equal(cleanNick('a   b'), 'a b');
  assert.equal(cleanNick('  דנה  '), 'דנה');
  assert.equal(cleanNick('Dana דנה'), 'Dana דנה');
});

test('cleanNick enforces length by characters, not bytes', () => {
  assert.equal(cleanNick(''), null);
  assert.equal(cleanNick('   '), null);
  assert.equal(cleanNick('x'.repeat(20)), 'x'.repeat(20));
  assert.equal(cleanNick('x'.repeat(21)), null);
  assert.equal(cleanNick('🏀'.repeat(20)), '🏀'.repeat(20)); // 20 code points, 80 bytes
  assert.equal(cleanNick('🏀'.repeat(21)), null);
});

test('cleanNick rejects control and bidi override characters and non-strings', () => {
  assert.equal(cleanNick('a\u0000b'), null);
  assert.equal(cleanNick('a\nb'), null);
  assert.equal(cleanNick('evil\u202eeno'), null); // right-to-left override
  assert.equal(cleanNick('a\u2066b'), null); // isolate
  assert.equal(cleanNick(' a\n '), null); // control char at an edge that trim would otherwise remove
  assert.equal(cleanNick('a\tb'), null);
  assert.equal(cleanNick('a\u007fb'), null); // DEL
  assert.equal(cleanNick('\u202eabc'), null); // leading right-to-left override
  assert.equal(cleanNick('a\u061cb'), null); // Arabic letter mark
  assert.equal(cleanNick(42), null);
  assert.equal(cleanNick(undefined), null);
});

test('hello is validated', () => {
  assert.deepEqual(parseClientMessage(msg({ type: 'hello', id: ID, nick: ' Dana ' })), {
    kind: 'hello', id: ID, nick: 'Dana',
  });
  assert.equal(parseClientMessage(msg({ type: 'hello', id: 'short', nick: 'Dana' })).reason, 'bad-id');
  assert.equal(parseClientMessage(msg({ type: 'hello', id: ID + '!', nick: 'Dana' })).reason, 'bad-id');
  assert.equal(parseClientMessage(msg({ type: 'hello', id: ID, nick: '' })).reason, 'bad-nick');
});

test('status is validated', () => {
  assert.deepEqual(parseClientMessage(msg({ type: 'status', status: 'in-match' })), { kind: 'status', status: 'in-match' });
  assert.equal(parseClientMessage(msg({ type: 'status', status: 'lobby' })).kind, 'status');
  assert.equal(parseClientMessage(msg({ type: 'status', status: 'away' })).reason, 'bad-status');
});

test('signal needs a target id and an object payload', () => {
  const ok = parseClientMessage(msg({ type: 'signal', to: ID, data: { kind: 'offer', sdp: 'v=0' } }));
  assert.deepEqual(ok, { kind: 'signal', to: ID, data: { kind: 'offer', sdp: 'v=0' } });
  assert.equal(parseClientMessage(msg({ type: 'signal', to: 'x', data: {} })).reason, 'bad-target');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: 'str' })).reason, 'bad-signal');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: [1] })).reason, 'bad-signal');
  assert.equal(parseClientMessage(msg({ type: 'signal', to: ID, data: null })).reason, 'bad-signal');
});

test('malformed input is rejected with a reason', () => {
  assert.equal(parseClientMessage(new ArrayBuffer(4)).reason, 'binary-not-allowed');
  assert.equal(parseClientMessage('{nope').reason, 'invalid-json');
  assert.equal(parseClientMessage('[]').reason, 'invalid-message');
  assert.equal(parseClientMessage('null').reason, 'invalid-message');
  assert.equal(parseClientMessage(msg({ type: 'chat', text: 'hi' })).reason, 'unknown-type');
  const big = msg({ type: 'signal', to: ID, data: { sdp: 'x'.repeat(MAX_LOBBY_MESSAGE_BYTES) } });
  assert.equal(parseClientMessage(big).reason, 'message-too-large');
});
