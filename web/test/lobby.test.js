import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { Lobby } from '../lobby.js';

const ME = 'MMMMMMMMMMMM';
const LOW = 'AAAAAAAAAAAA'; // sorts before ME: it makes the offer to us
const HIGH = 'ZZZZZZZZZZZZ'; // sorts after ME: we make the offer to it

class FakeWS {
  static all = [];
  constructor(url) {
    this.url = url;
    this.sent = [];
    this.readyState = 1;
    FakeWS.all.push(this);
  }
  send(t) {
    this.sent.push(JSON.parse(t));
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  push(obj) {
    this.onmessage?.({ data: JSON.stringify(obj) });
  }
}
class FakeDC {
  constructor() {
    this.readyState = 'connecting';
    this.sent = [];
  }
  send(t) {
    this.sent.push(JSON.parse(t));
  }
  close() {
    this.readyState = 'closed';
  }
  open() {
    this.readyState = 'open';
    this.onopen?.();
  }
}
class FakePC {
  static all = [];
  constructor() {
    this.offers = 0;
    this.remoteDescription = null;
    this.dc = null;
    FakePC.all.push(this);
  }
  createDataChannel() {
    this.dc = new FakeDC();
    return this.dc;
  }
  async createOffer() {
    this.offers += 1;
    return { type: 'offer', sdp: 'o' };
  }
  async createAnswer() {
    return { type: 'answer', sdp: 'a' };
  }
  async setLocalDescription(d) {
    this.localDescription = d;
  }
  async setRemoteDescription(d) {
    this.remoteDescription = d;
  }
  async addIceCandidate() {}
  close() {}
}

let timers;
let ids;
let events;
let lobby;
const tick = () => new Promise((r) => setImmediate(r));
const lastRoster = () => events.rosters.at(-1);
const link = (id) => lastRoster().find((p) => p.id === id)?.link;
const pcFor = (index) => FakePC.all[index];

function make() {
  FakeWS.all = [];
  FakePC.all = [];
  timers = [];
  ids = [ME, 'NNNNNNNNNNNN', 'OOOOOOOOOOOO'];
  events = { rosters: [], chats: [], states: [], peerMessages: [], left: [] };
  lobby = new Lobby(
    'https://signal.example',
    'Dana',
    {
      onState: (s) => events.states.push(s.phase),
      onRoster: (r) => events.rosters.push(r),
      onChat: (c) => events.chats.push(c),
      onPeerMessage: (id, m) => events.peerMessages.push([id, m]),
      onPeerLeft: (id) => events.left.push(id),
    },
    {
      WebSocket: FakeWS,
      RTCPeerConnection: FakePC,
      setTimeout: (fn, ms) => (timers.push({ fn, ms }), timers.length),
      clearTimeout: () => {},
      now: () => 0,
      randomId: () => ids.shift(),
    },
  );
  lobby.connect();
  FakeWS.all[0].onopen();
}

const welcome = (...others) => ({
  type: 'welcome',
  you: { id: ME, nick: 'Dana', status: 'lobby' },
  roster: [{ id: ME, nick: 'Dana', status: 'lobby' }, ...others.map((id) => ({ id, nick: id.slice(0, 3), status: 'lobby' }))],
});

beforeEach(() => {
  console.warn = () => {};
  make();
});

test('connect sends hello and welcome builds a roster with self first', async () => {
  const ws = FakeWS.all[0];
  assert.equal(ws.url, 'wss://signal.example/lobby/ws');
  assert.deepEqual(ws.sent[0], { type: 'hello', id: ME, nick: 'Dana' });
  ws.push(welcome(LOW, HIGH));
  await tick();
  const roster = lastRoster();
  assert.equal(roster[0].link, 'self');
  assert.equal(roster[0].name, 'Dana#MMM');
  assert.deepEqual(roster.slice(1).map((p) => p.id).sort(), [LOW, HIGH]);
  assert.equal(events.states.at(-1), 'online');
});

test('only the lower id makes the offer', async () => {
  FakeWS.all[0].push(welcome(LOW, HIGH));
  await tick();
  const offered = FakePC.all.filter((pc) => pc.offers > 0);
  assert.equal(offered.length, 1);
  const sentOffer = FakeWS.all[0].sent.find((m) => m.type === 'signal' && m.data.kind === 'offer');
  assert.equal(sentOffer.to, HIGH);
});

test('a data channel opening marks the player connected, and chat goes only to open peers', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  const [pcLow, pcHigh] = FakePC.all; // created in roster order
  pcLow.dc.open();
  assert.equal(link(LOW), 'open');
  assert.equal(link(HIGH), 'connecting');
  assert.equal(lobby.chat('שלום'), 'ok');
  assert.deepEqual(pcLow.dc.sent, [{ type: 'chat', text: 'שלום' }]);
  assert.deepEqual(pcHigh.dc.sent, []);
  assert.deepEqual(events.chats.at(-1), { from: { id: ME, name: 'Dana#MMM' }, text: 'שלום', mine: true });
});

test('an unreachable peer is marked, retried once, and chat to others keeps working', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  const [pcLow, pcHigh] = FakePC.all;
  pcLow.dc.open();
  // The connect timeout for HIGH fires: it never connected.
  const connectTimers = timers.filter((t) => t.ms === 20_000);
  assert.equal(connectTimers.length, 2);
  connectTimers[1].fn();
  assert.equal(link(HIGH), 'unreachable');
  assert.equal(link(LOW), 'open');
  assert.equal(lobby.chat('still here'), 'ok');
  assert.equal(pcLow.dc.sent.at(-1).text, 'still here');
  // We are the offerer for HIGH, so we retry once after a pause.
  const retry = timers.find((t) => t.ms === 2000);
  assert.ok(retry, 'a retry should be scheduled');
  retry.fn();
  assert.equal(link(HIGH), 'connecting');
  assert.equal(FakePC.all.length, 3);
  assert.notEqual(FakePC.all[2], pcHigh);
  // A second failure is not retried again.
  timers.filter((t) => t.ms === 20_000).at(-1).fn();
  assert.equal(link(HIGH), 'unreachable');
  assert.equal(timers.filter((t) => t.ms === 2000).length, 1);
});

test('the 17th player is told the lobby is full and does not reconnect in a loop', async () => {
  const ws = FakeWS.all[0];
  ws.push({ type: 'full' });
  ws.close();
  assert.equal(events.states.at(-1), 'full');
  assert.equal(timers.some((t) => t.ms <= 15_000 && t.ms >= 1000), false);
  assert.equal(FakeWS.all.length, 1);
});

test('losing the presence socket reconnects with a new id', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.close();
  assert.equal(events.states.at(-1), 'reconnecting');
  const reconnect = timers.find((t) => t.ms === 1000);
  assert.ok(reconnect);
  reconnect.fn();
  assert.equal(FakeWS.all.length, 2);
  FakeWS.all[1].onopen();
  assert.equal(FakeWS.all[1].sent[0].id, 'NNNNNNNNNNNN');
});

test('a flooding peer is dropped after the burst, and bad or oversized messages are ignored', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  const dc = FakePC.all[0].dc;
  dc.open();
  const chat = JSON.stringify({ type: 'chat', text: 'hi' });
  for (let i = 0; i < 40; i++) dc.onmessage({ data: chat });
  assert.equal(events.chats.filter((c) => !c.mine).length, 10); // burst of 10, clock frozen
  dc.onmessage({ data: JSON.stringify({ type: 'chat', text: 'x'.repeat(5000) }) });
  dc.onmessage({ data: 'garbage' });
  dc.onmessage({ data: new ArrayBuffer(8) });
  assert.equal(events.chats.filter((c) => !c.mine).length, 10);
});

test('muting a player silences their chat but not other messages', async () => {
  FakeWS.all[0].push(welcome(LOW));
  await tick();
  const dc = FakePC.all[0].dc;
  dc.open();
  lobby.mute(LOW, true);
  dc.onmessage({ data: JSON.stringify({ type: 'chat', text: 'hi' }) });
  assert.equal(events.chats.filter((c) => !c.mine).length, 0);
  dc.onmessage({ data: JSON.stringify({ type: 'decline' }) });
  assert.deepEqual(events.peerMessages.at(-1), [LOW, { type: 'decline' }]);
  assert.equal(lastRoster().find((p) => p.id === LOW).muted, true);
});

test('outgoing chat is rate limited and empty chat is refused', async () => {
  FakeWS.all[0].push(welcome(LOW));
  await tick();
  FakePC.all[0].dc.open();
  assert.equal(lobby.chat('   '), 'empty');
  assert.equal(lobby.chat('one'), 'ok');
  assert.equal(lobby.chat('two'), 'ok');
  assert.equal(lobby.chat('three'), 'rate'); // burst of 2, clock frozen
});

test('left removes the player and the peer, and tells the handler', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW, HIGH));
  await tick();
  ws.push({ type: 'left', id: HIGH });
  assert.equal(lastRoster().some((p) => p.id === HIGH), false);
  assert.deepEqual(events.left, [HIGH]);
});

test('status updates change the roster', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.push({ type: 'status', id: LOW, status: 'in-match' });
  assert.equal(lastRoster().find((p) => p.id === LOW).status, 'in-match');
});

test('sendTo reaches an open peer only', async () => {
  FakeWS.all[0].push(welcome(LOW, HIGH));
  await tick();
  FakePC.all[0].dc.open();
  assert.equal(lobby.sendTo(LOW, { type: 'decline' }), true);
  assert.equal(lobby.sendTo(HIGH, { type: 'decline' }), false);
  assert.equal(lobby.sendTo('nobody', { type: 'decline' }), false);
});

test('an incoming offer builds an answering peer that replies with an answer', async () => {
  const ws = FakeWS.all[0];
  ws.push(welcome(LOW));
  await tick();
  ws.push({ type: 'signal', from: LOW, data: { kind: 'offer', sdp: 'remote-offer' } });
  await tick();
  const answer = ws.sent.find((m) => m.type === 'signal' && m.data.kind === 'answer');
  assert.deepEqual(answer, { type: 'signal', to: LOW, data: { kind: 'answer', sdp: 'a' } });
  assert.equal(FakePC.all.at(-1).remoteDescription.sdp, 'remote-offer');
});
