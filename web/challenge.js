// Challenge state, as a pure reducer: (state, event, context) -> { state, effects }.
// `out` is the challenge we sent, `inc` the one we are being asked about. Effects are plain
// objects the page carries out (send a message, host a room, show a prompt, ...).

export const CHALLENGE_TIMEOUT_MS = 30_000;

export const initialChallenges = () => ({ out: null, inc: null });

const decline = (to) => ({ send: { to, msg: { type: 'decline' } } });

export function reduce(state, ev, ctx = { busy: false }) {
  const { out, inc } = state;
  switch (ev.type) {
    case 'start':
      if (out || inc || ctx.busy) return { state, effects: [{ notify: 'busy' }] };
      return { state: { out: { to: ev.to, phase: 'creating' }, inc }, effects: [{ host: true }] };

    case 'room':
      if (out?.phase !== 'creating') return { state, effects: [{ cancelHost: true }] }; // cancelled meanwhile
      return {
        state: { out: { ...out, phase: 'sent', room: ev.room }, inc },
        effects: [{ send: { to: out.to, msg: { type: 'challenge', room: ev.room } } }, { timer: 'out' }],
      };

    case 'msg':
      return onMessage(state, ev, ctx);

    case 'accept-in':
      if (!inc) return { state, effects: [] };
      if (ctx.busy) {
        // Became busy while the prompt was up (hosting or joining a room code): joining now would
        // run two connections at once.
        return { state: { out, inc: null }, effects: [decline(inc.from), { closePrompt: true }, { clearTimer: 'inc' }, { notify: 'busy' }] };
      }
      return {
        state: { out, inc: null },
        effects: [{ send: { to: inc.from, msg: { type: 'accept' } } }, { join: inc.room }, { closePrompt: true }, { clearTimer: 'inc' }],
      };

    case 'decline-in':
      if (!inc) return { state, effects: [] };
      return { state: { out, inc: null }, effects: [decline(inc.from), { closePrompt: true }, { clearTimer: 'inc' }] };

    case 'timeout':
      if (ev.which === 'out' && out?.phase === 'sent') {
        return {
          state: { out: null, inc },
          effects: [{ send: { to: out.to, msg: { type: 'cancel' } } }, { cancelHost: true }, { notify: 'no-answer' }],
        };
      }
      if (ev.which === 'out' && out?.phase === 'accepted') {
        // They said yes but never connected to the room.
        return {
          state: { out: null, inc },
          effects: [{ send: { to: out.to, msg: { type: 'cancel' } } }, { cancelHost: true }, { notify: 'no-connect' }],
        };
      }
      if (ev.which === 'inc' && inc) {
        return { state: { out, inc: null }, effects: [decline(inc.from), { closePrompt: true }] };
      }
      return { state, effects: [] };

    case 'peer-left': {
      const effects = [];
      let next = state;
      // Once accepted, the room handshake is under way and independent of the presence socket; a
      // blip must not kill it. The accepted-phase timeout covers a target that really vanished.
      if (out?.to === ev.id && out.phase !== 'accepted') {
        next = { ...next, out: null };
        effects.push({ cancelHost: true }, { clearTimer: 'out' });
      }
      if (inc?.from === ev.id) {
        next = { ...next, inc: null };
        effects.push({ closePrompt: true }, { clearTimer: 'inc' });
      }
      return { state: next, effects };
    }

    case 'abandon': // we gave up our own challenge (Leave, or the room link failed)
      if (!out) return { state, effects: [] };
      return { state: { out: null, inc }, effects: [{ cancelHost: true }, { clearTimer: 'out' }] };

    case 'match-started':
      return { state: initialChallenges(), effects: [{ clearTimer: 'out' }, { clearTimer: 'inc' }, { closePrompt: true }] };

    case 'hosting-failed':
      return { state: { out: null, inc }, effects: [{ notify: 'hosting-failed' }] };

    default:
      return { state, effects: [] };
  }
}

function onMessage(state, { from, msg }, ctx) {
  const { out, inc } = state;
  switch (msg.type) {
    case 'challenge':
      if (out?.to === from) {
        // Each of us challenged the other at once: cancel both and let them try again.
        return {
          state: { out: null, inc },
          effects: [{ cancelHost: true }, { clearTimer: 'out' }, decline(from), { notify: 'simultaneous' }],
        };
      }
      if (out || inc || ctx.busy) return { state, effects: [decline(from)] };
      return { state: { out, inc: { from, room: msg.room } }, effects: [{ prompt: { from, room: msg.room } }, { timer: 'inc' }] };

    case 'accept':
      if (out?.to !== from || out.phase !== 'sent') return { state, effects: [] };
      return { state: { out: { ...out, phase: 'accepted' }, inc }, effects: [{ timer: 'out' }, { notify: 'accepted' }] };

    case 'decline':
      if (out?.to !== from) return { state, effects: [] };
      return { state: { out: null, inc }, effects: [{ cancelHost: true }, { clearTimer: 'out' }, { notify: 'declined' }] };

    case 'cancel':
      if (inc?.from !== from) return { state, effects: [] };
      return { state: { out, inc: null }, effects: [{ closePrompt: true }, { clearTimer: 'inc' }] };

    default:
      return { state, effects: [] };
  }
}
