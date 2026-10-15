// Runs two copies of the built wasm against each other through the same exports the browser
// uses (alloc/push/pop/step), with a lossy, delayed in-memory "network" between them.
// Usage: node scripts/test-net-wasm.mjs [web/ntba.wasm]
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const bytes = readFileSync(process.argv[2] ?? 'web/ntba.wasm');
const module = await WebAssembly.compile(bytes);

async function peer() {
  const { exports: w } = await WebAssembly.instantiate(module, {});
  w.ntba_init(1);
  const logs = [];
  return {
    w,
    logs,
    drainLogs() {
      for (let n; (n = w.ntba_log_next() >>> 0) !== 0xffffffff; ) {
        logs.push([w.ntba_log_level(), new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.ntba_log_ptr(), n))]);
      }
    },
    receive(data) {
      const ptr = w.ntba_net_alloc(data.length);
      new Uint8Array(w.memory.buffer, ptr, data.length).set(data);
      w.ntba_net_push();
    },
    outgoing() {
      const out = [];
      for (let n; (n = w.ntba_net_pop() >>> 0) !== 0xffffffff; ) {
        const data = new Uint8Array(w.memory.buffer, w.ntba_net_out_ptr(), n).slice();
        out.push({ data, unreliable: w.ntba_net_out_channel() === 1 });
      }
      return out;
    },
    hash: () => (BigInt(w.ntba_state_hash_hi() >>> 0) << 32n) | BigInt(w.ntba_state_hash_lo() >>> 0),
  };
}

const [host, guest] = [await peer(), await peer()];
host.w.ntba_net_start(0, 4242, 6);
guest.w.ntba_net_start(1, 0, 0);

// A tiny network: 3-frame latency, 20% loss on the unreliable channel, in-order otherwise.
const wire = { toGuest: [], toHost: [] };
let seed = 12345;
const rand = () => ((seed = (Math.imul(seed, 1103515245) + 12345) >>> 0) / 2 ** 32);
const send = (queue, msgs, frame) => {
  for (const m of msgs) if (!(m.unreliable && rand() < 0.2)) queue.push({ at: frame + 3, data: m.data });
};
const deliver = (queue, to, frame) => {
  while (queue.length && queue[0].at <= frame) to.receive(queue.shift().data);
};

const FRAMES = 1500;
let stepped = 0;
for (let frame = 0; frame < FRAMES; frame++) {
  deliver(wire.toGuest, guest, frame);
  deliver(wire.toHost, host, frame);
  // Both players mash buttons deterministically.
  const hostPad = (frame * 7) & 0x7f;
  const guestPad = (frame * 13 + 5) & 0x7f;
  if (host.w.ntba_net_step(hostPad) === 1) stepped++;
  guest.w.ntba_net_step(guestPad);
  send(wire.toGuest, host.outgoing(), frame);
  send(wire.toHost, guest.outgoing(), frame);
}
host.drainLogs();
guest.drainLogs();

assert.equal(host.w.ntba_net_status(), 2, 'host running');
assert.equal(guest.w.ntba_net_status(), 2, 'guest running');
assert.equal(host.w.ntba_net_side(), 0);
assert.equal(guest.w.ntba_net_side(), 1);
assert.ok(stepped > 1000, `host only stepped ${stepped} ticks`);
assert.ok((host.w.ntba_net_rtt() >>> 0) < 30, 'rtt measured');
assert.ok(guest.logs.some(([l, m]) => l === 1 && m.includes('connected as')), 'guest logged the connection at INFO');

// The two simulations agree. They may be a few ticks apart, so run the one that is behind
// until the ticks match, then compare hashes.
for (let i = 0; i < 20 && host.w.ntba_net_tick() !== guest.w.ntba_net_tick(); i++) {
  deliver(wire.toGuest, guest, 1e9);
  deliver(wire.toHost, host, 1e9);
  host.w.ntba_net_step(0);
  guest.w.ntba_net_step(0);
  send(wire.toGuest, host.outgoing(), 0);
  send(wire.toHost, guest.outgoing(), 0);
}
assert.equal(host.w.ntba_net_tick(), guest.w.ntba_net_tick(), 'peers could not be brought to the same tick');
assert.equal(host.hash(), guest.hash(), 'simulations diverged');
console.log(`ticks host=${host.w.ntba_net_tick()} guest=${guest.w.ntba_net_tick()}`);

// Quit: the peer finds out.
host.w.ntba_net_stop();
for (const m of host.outgoing()) guest.receive(m.data);
guest.w.ntba_net_step(0);
assert.equal(host.w.ntba_net_end_reason(), 2, 'host: local quit');
assert.equal(guest.w.ntba_net_end_reason(), 1, 'guest: peer quit');
console.log('wasm net glue ok');
