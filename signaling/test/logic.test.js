import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  CODE_ALPHABET,
  CODE_LENGTH,
  MAX_MESSAGE_BYTES,
  classifyMessage,
  generateCode,
  normaliseCode,
  originAllowed,
  parseRoomWsPath,
  ttlSeconds,
} from '../src/logic.js';

test('generated codes use only the unambiguous alphabet', () => {
  for (let i = 0; i < 500; i++) {
    const code = generateCode();
    assert.equal(code.length, CODE_LENGTH);
    assert.ok([...code].every((c) => CODE_ALPHABET.includes(c)), code);
  }
  assert.ok(!/[01OIL]/.test(CODE_ALPHABET));
});

test('code generation has no modulo bias: bytes above the cut-off are rejected', () => {
  // 31 symbols: the largest multiple of 31 below 256 is 248, so 248..255 must be skipped.
  const stream = [255, 254, 253, 252, 251, 250, 249, 248, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
  let pos = 0;
  const code = generateCode((a) => {
    for (let i = 0; i < a.length; i++) a[i] = stream[pos++ % stream.length];
    return a;
  });
  assert.equal(code, [...'ABCDEF'].join(''));
});

test('codes are normalised and malformed ones rejected', () => {
  assert.equal(normaliseCode('abc-234'), 'ABC234');
  assert.equal(normaliseCode(' ab c2 34 '), 'ABC234');
  for (const bad of ['', 'ABC', 'ABCDEFG', 'ABC10O', 'AB CDE!', null, undefined]) {
    assert.equal(normaliseCode(bad), null, String(bad));
  }
});

test('room websocket paths', () => {
  assert.equal(parseRoomWsPath('/rooms/abc234/ws'), 'ABC234');
  assert.equal(parseRoomWsPath('/rooms/ABC234/ws/extra'), null);
  assert.equal(parseRoomWsPath('/rooms/%E0%A4%A/ws'), null);
  assert.equal(parseRoomWsPath('/rooms/ABC234'), null);
  assert.equal(parseRoomWsPath('/rooms//ws'), null);
});

test('origin allow-list', () => {
  const list = 'https://eyalro.github.io, http://localhost:8080';
  assert.ok(originAllowed('https://eyalro.github.io', list));
  assert.ok(originAllowed('http://localhost:8080', list));
  assert.ok(!originAllowed('https://evil.example', list));
  assert.ok(!originAllowed('https://eyalro.github.io.evil.example', list));
  assert.ok(originAllowed(null, list)); // non-browser clients
  assert.ok(!originAllowed('https://eyalro.github.io', ''));
});

test('messages are classified strictly', () => {
  assert.deepEqual(classifyMessage('{"type":"offer","sdp":"x"}'), {
    kind: 'relay',
    text: '{"type":"offer","sdp":"x"}',
  });
  assert.equal(classifyMessage('{"type":"answer"}').kind, 'relay');
  assert.equal(classifyMessage('{"type":"ice","candidate":{}}').kind, 'relay');
  assert.deepEqual(classifyMessage('{"type":"done"}'), { kind: 'control', type: 'done' });
  const reason = (d) => classifyMessage(d).reason;
  assert.equal(reason(new ArrayBuffer(4)), 'binary-not-allowed');
  assert.equal(reason('not json'), 'invalid-json');
  assert.equal(reason('[1,2]'), 'invalid-message');
  assert.equal(reason('null'), 'invalid-message');
  assert.equal(reason('{"type":"rm -rf"}'), 'unknown-type');
  assert.equal(reason('{}'), 'unknown-type');
  const big = JSON.stringify({ type: 'offer', sdp: 'x'.repeat(MAX_MESSAGE_BYTES) });
  assert.equal(reason(big), 'message-too-large');
});

test('ttl is bounded', () => {
  assert.equal(ttlSeconds('600'), 600);
  assert.equal(ttlSeconds('5'), 600);
  assert.equal(ttlSeconds('999999'), 600);
  assert.equal(ttlSeconds(undefined), 600);
  assert.equal(ttlSeconds('120'), 120);
});
