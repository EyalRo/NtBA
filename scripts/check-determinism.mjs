// Proves the built wasm simulates bit-identically to the reference hash pinned in
// crates/ntba-core/src/replay.rs (the native test pins the same value).
// Usage: node scripts/check-determinism.mjs [web/ntba.wasm]
import { readFileSync } from 'node:fs';

const path = process.argv[2] ?? 'web/ntba.wasm';
const { instance } = await WebAssembly.instantiate(readFileSync(path), {});
const w = instance.exports;
const join = (hi, lo) => (BigInt(hi >>> 0) << 32n) | BigInt(lo >>> 0);
const got = join(w.ntba_determinism_hash_hi(), w.ntba_determinism_hash_lo());
const want = join(w.ntba_determinism_expected_hi(), w.ntba_determinism_expected_lo());
const hex = (v) => `0x${v.toString(16).padStart(16, '0')}`;
if (got !== want) {
  console.error(`determinism check FAILED: wasm ${hex(got)} != expected ${hex(want)}`);
  process.exit(1);
}
console.log(`determinism check ok: ${hex(got)}`);
