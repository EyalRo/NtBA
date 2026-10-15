//! Delay-based lock-step session.
//!
//! Both peers run the same [`Game`]. Each tick needs both teams' pad bytes; a peer's own
//! input for tick `t` is sampled `input_delay` ticks early so it can cross the network in
//! time. When the remote byte for the next tick has not arrived the session *stalls* (the
//! host simply does not step) instead of guessing. Inputs travel on the unreliable channel as
//! a window of every byte the peer has not acknowledged yet, so a lost packet is repaired by
//! the next one without waiting for retransmission.
//!
//! Every [`HASH_INTERVAL`] ticks the peers exchange a state hash; a mismatch ends the match
//! instead of letting two diverging games carry on.

use std::collections::BTreeMap;

use ntba_core::game::Game;
use ntba_core::replay::{InputLog, DETERMINISM_SEED_HASH};

use crate::protocol::{ByeReason, Message, MAX_INPUTS_PER_MESSAGE, PROTOCOL_VERSION};
use crate::transport::{Channel, Transport};
use crate::{net_log, Level};

/// Ticks between state-hash comparisons.
pub const HASH_INTERVAL: u32 = 60;
/// Frames of silence after which the peer is considered gone (5 s at 60 Hz).
pub const PEER_TIMEOUT_FRAMES: u32 = 300;
/// Frames between pings.
const PING_EVERY: u32 = 30;
/// Largest input delay the session accepts from a peer (about 0.4 s).
pub const MAX_INPUT_DELAY: u8 = 24;

const LOG: &str = "ntba_net::session";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Owns the seed and the input delay; plays the home team.
    Host,
    /// Adopts the host's seed and input delay; plays the away team.
    Guest,
}

/// Why the match is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The peer left on purpose.
    PeerQuit,
    /// We left on purpose.
    LocalQuit,
    /// The simulations diverged at `tick`.
    Desync { tick: u32, local: u64, remote: u64 },
    /// Different protocol or game build.
    VersionMismatch,
    /// The transport closed or failed.
    Disconnected,
    /// The peer stopped sending.
    Timeout,
    /// The peer sent something that is not valid.
    ProtocolError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Exchanging `Hello`.
    Connecting,
    Running,
    Ended(EndReason),
}

/// What one [`Session::update`] call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Connecting,
    /// The simulation advanced one tick.
    Stepped,
    /// Waiting for the peer's input for the next tick.
    Stalled,
    Ended(EndReason),
}

pub struct Session<T: Transport> {
    transport: T,
    role: Role,
    side: usize,
    input_delay: u32,
    game: Option<Game>,
    status: Status,
    got_hello: bool,
    /// Own pad bytes by tick (sampled ahead of the simulation).
    local: Vec<u8>,
    /// Peer pad bytes by tick; always contiguous from tick 0.
    remote: Vec<u8>,
    /// How many of our bytes the peer has confirmed receiving.
    peer_ack: u32,
    log: InputLog,
    local_hashes: BTreeMap<u32, u64>,
    remote_hashes: BTreeMap<u32, u64>,
    frames: u32,
    last_heard: u32,
    rtt_frames: Option<u32>,
    seed: u32,
}

impl<T: Transport> Session<T> {
    /// Start as the host. `seed` fixes the whole match; `input_delay` is in ticks.
    pub fn host(transport: T, seed: u32, input_delay: u8) -> Self {
        let mut s = Self::new(
            transport,
            Role::Host,
            seed,
            input_delay.min(MAX_INPUT_DELAY),
        );
        s.start_game(seed);
        s.send_hello();
        s
    }

    /// Start as the guest; the seed and delay come from the host's `Hello`.
    pub fn guest(transport: T) -> Self {
        let mut s = Self::new(transport, Role::Guest, 0, 0);
        s.send_hello();
        s
    }

    fn new(transport: T, role: Role, seed: u32, input_delay: u8) -> Self {
        Session {
            transport,
            role,
            side: if role == Role::Host { 0 } else { 1 },
            input_delay: input_delay as u32,
            game: None,
            status: Status::Connecting,
            got_hello: false,
            local: Vec::new(),
            remote: Vec::new(),
            peer_ack: 0,
            log: InputLog::new(seed),
            local_hashes: BTreeMap::new(),
            remote_hashes: BTreeMap::new(),
            frames: 0,
            last_heard: 0,
            rtt_frames: None,
            seed,
        }
    }

    fn start_game(&mut self, seed: u32) {
        self.seed = seed;
        self.log = InputLog::new(seed);
        self.game = Some(Game::new(seed));
        // The first `input_delay` ticks have no sampled input yet; both peers use zeros.
        self.local = vec![0; self.input_delay as usize];
        self.remote = vec![0; self.input_delay as usize];
    }

    fn send_hello(&mut self) {
        let hello = Message::Hello {
            protocol: PROTOCOL_VERSION,
            build: DETERMINISM_SEED_HASH,
            seed: self.seed,
            side: (1 - self.side) as u8,
            input_delay: self.input_delay as u8,
        };
        self.send(&hello);
    }

    fn send(&mut self, msg: &Message) {
        let channel = if msg.is_reliable() {
            Channel::Reliable
        } else {
            Channel::Unreliable
        };
        self.transport.send(channel, &msg.encode());
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The team this peer controls (0 = home, 1 = away).
    pub fn side(&self) -> usize {
        self.side
    }

    pub fn input_delay(&self) -> u32 {
        self.input_delay
    }

    /// The simulation, once the handshake has produced one.
    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// Ticks simulated so far.
    pub fn tick(&self) -> u32 {
        self.log.len() as u32
    }

    /// Round-trip time in frames, once a ping has been answered.
    pub fn rtt_frames(&self) -> Option<u32> {
        self.rtt_frames
    }

    /// Both teams' pads for every simulated tick; a complete, replayable recording.
    pub fn input_log(&self) -> &InputLog {
        &self.log
    }

    /// Leave on purpose and tell the peer.
    pub fn quit(&mut self) {
        if !matches!(self.status, Status::Ended(_)) {
            self.send(&Message::Bye {
                reason: ByeReason::Quit,
            });
            self.end(EndReason::LocalQuit);
        }
    }

    fn end(&mut self, reason: EndReason) {
        if matches!(self.status, Status::Ended(_)) {
            return;
        }
        let level = match reason {
            EndReason::PeerQuit | EndReason::LocalQuit => Level::Info,
            _ => Level::Error,
        };
        net_log!(
            level,
            LOG,
            "match ended at tick {}: {reason:?}",
            self.tick()
        );
        self.status = Status::Ended(reason);
        self.transport.close();
    }

    /// Call once per 1/60 s (and again while ticks are owed). `sample` reads the local pad
    /// and is invoked at most once, and only when a new input is actually needed, so a stall
    /// never silently grows the input delay.
    pub fn update(&mut self, sample: impl FnOnce() -> u8) -> Update {
        if let Status::Ended(reason) = self.status {
            return Update::Ended(reason);
        }
        self.frames += 1;
        self.receive();
        if let Status::Ended(reason) = self.status {
            return Update::Ended(reason);
        }
        if !self.transport.is_open() {
            self.end(EndReason::Disconnected);
            return Update::Ended(EndReason::Disconnected);
        }
        if self.frames - self.last_heard > PEER_TIMEOUT_FRAMES {
            self.end(EndReason::Timeout);
            return Update::Ended(EndReason::Timeout);
        }
        if self.game.is_none() || !self.got_hello {
            if self.frames % PING_EVERY == 0 {
                // Hello rides a reliable channel, so this is only a keep-alive for lossy setups.
                self.send_hello();
            }
            return Update::Connecting;
        }
        self.status = Status::Running;

        let tick = self.tick();
        let wanted = tick as usize + self.input_delay as usize + 1;
        if self.local.len() < wanted {
            self.local.push(sample());
        }
        self.send_inputs();
        if self.frames % PING_EVERY == 0 {
            self.send(&Message::Ping { id: self.frames });
        }

        if self.remote.len() <= tick as usize {
            return Update::Stalled;
        }
        let (mine, theirs) = (self.local[tick as usize], self.remote[tick as usize]);
        let pads = if self.side == 0 {
            [mine, theirs]
        } else {
            [theirs, mine]
        };
        let game = self.game.as_mut().expect("checked above");
        game.step(pads);
        self.log.push(pads);
        let done = self.log.len() as u32;
        if done % HASH_INTERVAL == 0 {
            let hash = game.state_hash();
            self.local_hashes.insert(done, hash);
            self.send(&Message::Hash { tick: done, hash });
            self.compare_hashes();
        }
        match self.status {
            Status::Ended(reason) => Update::Ended(reason),
            _ => Update::Stepped,
        }
    }

    fn send_inputs(&mut self) {
        let from = self.peer_ack as usize;
        let to = self.local.len().min(from + MAX_INPUTS_PER_MESSAGE);
        let msg = Message::Inputs {
            ack: self.remote.len() as u32,
            first_tick: self.peer_ack,
            pads: self.local[from..to].to_vec(),
        };
        self.send(&msg);
    }

    fn receive(&mut self) {
        while let Some(bytes) = self.transport.recv() {
            self.last_heard = self.frames;
            match Message::decode(&bytes) {
                Ok(msg) => self.handle(msg),
                Err(err) => {
                    net_log!(Level::Warn, LOG, "dropping undecodable message: {err}");
                    // Corrupt reliable data means the stream cannot be trusted any more.
                    if self.status == Status::Connecting {
                        self.end(EndReason::ProtocolError);
                    }
                }
            }
            if matches!(self.status, Status::Ended(_)) {
                return;
            }
        }
    }

    fn handle(&mut self, msg: Message) {
        match msg {
            Message::Hello {
                protocol,
                build,
                seed,
                input_delay,
                ..
            } => self.on_hello(protocol, build, seed, input_delay),
            Message::Inputs {
                ack,
                first_tick,
                pads,
            } => {
                self.peer_ack = self.peer_ack.max(ack.min(self.local.len() as u32));
                for (i, byte) in pads.into_iter().enumerate() {
                    if first_tick as usize + i == self.remote.len() {
                        self.remote.push(byte);
                    }
                }
            }
            Message::Hash { tick, hash } => {
                self.remote_hashes.insert(tick, hash);
                self.compare_hashes();
            }
            Message::Ping { id } => self.send(&Message::Pong { id }),
            Message::Pong { id } => {
                self.rtt_frames = Some(self.frames.saturating_sub(id));
            }
            Message::Bye { reason } => self.end(match reason {
                ByeReason::Quit => EndReason::PeerQuit,
                ByeReason::VersionMismatch => EndReason::VersionMismatch,
                ByeReason::Desync => {
                    // The peer saw a mismatch first; report it with whatever we know.
                    let tick = self.tick();
                    EndReason::Desync {
                        tick,
                        local: self.local_hashes.get(&tick).copied().unwrap_or(0),
                        remote: 0,
                    }
                }
            }),
        }
    }

    fn on_hello(&mut self, protocol: u16, build: u64, seed: u32, input_delay: u8) {
        if self.got_hello {
            return;
        }
        if protocol != PROTOCOL_VERSION || build != DETERMINISM_SEED_HASH {
            net_log!(
                Level::Error,
                LOG,
                "peer is incompatible: protocol {protocol} (ours {PROTOCOL_VERSION}), build {build:#x} (ours {DETERMINISM_SEED_HASH:#x})"
            );
            self.send(&Message::Bye {
                reason: ByeReason::VersionMismatch,
            });
            self.end(EndReason::VersionMismatch);
            return;
        }
        if self.role == Role::Guest {
            if input_delay > MAX_INPUT_DELAY {
                net_log!(
                    Level::Error,
                    LOG,
                    "host asked for input delay {input_delay}"
                );
                self.end(EndReason::ProtocolError);
                return;
            }
            self.input_delay = input_delay as u32;
            self.start_game(seed);
        }
        net_log!(
            Level::Info,
            LOG,
            "connected as {:?}, side {}, input delay {} ticks, seed {}",
            self.role,
            self.side,
            self.input_delay,
            self.seed
        );
        self.got_hello = true;
    }

    fn compare_hashes(&mut self) {
        let ticks: Vec<u32> = self.local_hashes.keys().copied().collect();
        for tick in ticks {
            let (Some(&local), Some(&remote)) =
                (self.local_hashes.get(&tick), self.remote_hashes.get(&tick))
            else {
                continue;
            };
            self.local_hashes.remove(&tick);
            self.remote_hashes.remove(&tick);
            if local != remote {
                net_log!(
                    Level::Error,
                    LOG,
                    "desync at tick {tick}: local {local:#018x}, remote {remote:#018x}"
                );
                self.send(&Message::Bye {
                    reason: ByeReason::Desync,
                });
                self.end(EndReason::Desync {
                    tick,
                    local,
                    remote,
                });
                return;
            }
            net_log!(Level::Debug, LOG, "tick {tick}: state hashes agree");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{LocalLink, LocalTransport, NetConfig};
    use ntba_core::replay::InputLog;

    type Pair = (LocalLink, Session<LocalTransport>, Session<LocalTransport>);

    fn pair(cfg: NetConfig, seed: u32, delay: u8) -> Pair {
        let (link, ta, tb) = LocalLink::pair(cfg);
        (link, Session::host(ta, seed, delay), Session::guest(tb))
    }

    /// Per-side scripted pads: the `n`-th sample of side `s` is `script[n][s]`.
    struct Pads {
        script: InputLog,
        used: [usize; 2],
    }

    impl Pads {
        fn new(seed: u32) -> Self {
            Pads {
                script: InputLog::synthetic(seed, 20_000),
                used: [0; 2],
            }
        }

        fn sample(&mut self, side: usize) -> u8 {
            let pad = self.script.pads[self.used[side]][side];
            self.used[side] += 1;
            pad
        }
    }

    /// One frame for both peers; returns their updates.
    fn frame(
        link: &LocalLink,
        a: &mut Session<LocalTransport>,
        b: &mut Session<LocalTransport>,
        pads: &mut Pads,
    ) -> (Update, Update) {
        let ua = a.update(|| pads.sample(0));
        let ub = b.update(|| pads.sample(1));
        link.advance();
        (ua, ub)
    }

    fn run(
        link: &LocalLink,
        a: &mut Session<LocalTransport>,
        b: &mut Session<LocalTransport>,
        frames: u32,
    ) {
        let mut pads = Pads::new(77);
        for _ in 0..frames {
            frame(link, a, b, &mut pads);
        }
    }

    /// Both peers simulated the same prefix with identical inputs, and a fresh replay of that
    /// log gives the same state again.
    fn assert_in_sync(a: &Session<LocalTransport>, b: &Session<LocalTransport>) {
        let n = a.tick().min(b.tick()) as usize;
        assert!(n > 0, "nothing was simulated");
        assert_eq!(a.input_log().pads[..n], b.input_log().pads[..n]);
        // Each peer's live game equals a fresh replay of the inputs it consumed...
        for peer in [a, b] {
            assert_eq!(
                peer.game().unwrap().state_hash(),
                peer.input_log().replay().state_hash()
            );
        }
        // ...and the two peers agree with each other at their common tick.
        let at = |peer: &Session<LocalTransport>| {
            let mut log = peer.input_log().clone();
            log.pads.truncate(n);
            log.replay().state_hash()
        };
        assert_eq!(at(a), at(b));
        assert_eq!(a.input_log().seed, b.input_log().seed);
    }

    #[test]
    fn handshake_assigns_sides_and_adopts_host_settings() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 2,
                ..NetConfig::default()
            },
            1234,
            5,
        );
        assert_eq!(a.status(), Status::Connecting);
        run(&link, &mut a, &mut b, 30);
        assert_eq!(a.status(), Status::Running);
        assert_eq!(b.status(), Status::Running);
        assert_eq!((a.side(), b.side()), (0, 1));
        assert_eq!(b.input_delay(), 5);
        assert_eq!(b.input_log().seed, 1234);
    }

    #[test]
    fn stays_in_lock_step_on_a_clean_link() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 3,
                ..NetConfig::default()
            },
            9,
            6,
        );
        run(&link, &mut a, &mut b, 1_200);
        assert!(
            a.tick() > 1_000 && b.tick() > 1_000,
            "{} {}",
            a.tick(),
            b.tick()
        );
        assert_in_sync(&a, &b);
    }

    #[test]
    fn survives_jitter_and_packet_loss() {
        let cfg = NetConfig {
            latency: 4,
            jitter: 6,
            loss_percent: 25,
            seed: 3,
        };
        let (link, mut a, mut b) = pair(cfg, 21, 8);
        run(&link, &mut a, &mut b, 3_000);
        assert!(
            a.tick() > 2_000 && b.tick() > 2_000,
            "{} {}",
            a.tick(),
            b.tick()
        );
        assert_in_sync(&a, &b);
        assert_eq!(a.status(), Status::Running);
        assert_eq!(b.status(), Status::Running);
    }

    #[test]
    fn stalls_instead_of_guessing_when_the_peer_is_slow() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 20,
                ..NetConfig::default()
            },
            5,
            2,
        );
        let mut pads = Pads::new(5);
        let mut stalled = 0;
        for _ in 0..400 {
            if frame(&link, &mut a, &mut b, &mut pads).0 == Update::Stalled {
                stalled += 1;
            }
        }
        assert!(
            stalled > 0,
            "a 20-tick latency with a 2-tick delay must stall"
        );
        assert_in_sync(&a, &b);
        // Stalling never samples extra input: samples used == ticks simulated + delay (+1 pending).
        assert!(
            pads.used[0] <= a.tick() as usize + 3,
            "{} vs {}",
            pads.used[0],
            a.tick()
        );
    }

    #[test]
    fn measures_round_trip_time() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 5,
                ..NetConfig::default()
            },
            1,
            8,
        );
        run(&link, &mut a, &mut b, 200);
        let rtt = a.rtt_frames().expect("a ping was answered");
        assert!((10..=14).contains(&rtt), "rtt {rtt}");
    }

    #[test]
    fn a_diverging_simulation_is_detected() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 2,
                ..NetConfig::default()
            },
            8,
            4,
        );
        run(&link, &mut a, &mut b, 100);
        // Corrupt one peer's state behind the protocol's back.
        b.game.as_mut().unwrap().score[0] += 1;
        run(&link, &mut a, &mut b, 400);
        assert!(
            matches!(a.status(), Status::Ended(EndReason::Desync { .. })),
            "{:?}",
            a.status()
        );
        assert!(
            matches!(b.status(), Status::Ended(EndReason::Desync { .. })),
            "{:?}",
            b.status()
        );
    }

    #[test]
    fn different_builds_refuse_to_play() {
        let (link, ta, mut tb) = {
            let (l, a, b) = LocalLink::pair(NetConfig::default());
            (l, a, b)
        };
        let mut host = Session::host(ta, 1, 4);
        // A guest from another build: right protocol, wrong simulation hash.
        tb.send(
            Channel::Reliable,
            &Message::Hello {
                protocol: PROTOCOL_VERSION,
                build: DETERMINISM_SEED_HASH ^ 1,
                seed: 0,
                side: 0,
                input_delay: 0,
            }
            .encode(),
        );
        link.advance();
        assert_eq!(host.update(|| 0), Update::Ended(EndReason::VersionMismatch));
        link.advance();
        // The would-be guest was told why.
        let reply = Message::decode(&tb.recv().unwrap());
        assert!(matches!(reply, Ok(Message::Hello { .. })));
        let reply = Message::decode(&tb.recv().unwrap());
        assert_eq!(
            reply,
            Ok(Message::Bye {
                reason: ByeReason::VersionMismatch
            })
        );
    }

    #[test]
    fn protocol_mismatch_is_refused_too() {
        let (link, ta, mut tb) = LocalLink::pair(NetConfig::default());
        let mut host = Session::host(ta, 1, 4);
        tb.send(
            Channel::Reliable,
            &Message::Hello {
                protocol: PROTOCOL_VERSION + 1,
                build: DETERMINISM_SEED_HASH,
                seed: 0,
                side: 0,
                input_delay: 0,
            }
            .encode(),
        );
        link.advance();
        assert_eq!(host.update(|| 0), Update::Ended(EndReason::VersionMismatch));
    }

    #[test]
    fn quitting_ends_both_sides_cleanly() {
        let (link, mut a, mut b) = pair(
            NetConfig {
                latency: 1,
                ..NetConfig::default()
            },
            1,
            3,
        );
        run(&link, &mut a, &mut b, 60);
        a.quit();
        assert_eq!(a.status(), Status::Ended(EndReason::LocalQuit));
        link.advance();
        // The transport is closed by the quitter, but its Bye was queued first.
        let u = b.update(|| 0);
        assert!(
            matches!(
                u,
                Update::Ended(EndReason::PeerQuit | EndReason::Disconnected)
            ),
            "{u:?}"
        );
    }

    #[test]
    fn a_severed_connection_is_reported() {
        let (link, mut a, mut b) = pair(NetConfig::default(), 1, 3);
        run(&link, &mut a, &mut b, 30);
        link.sever();
        assert_eq!(a.update(|| 0), Update::Ended(EndReason::Disconnected));
        assert_eq!(b.update(|| 0), Update::Ended(EndReason::Disconnected));
    }

    #[test]
    fn a_silent_peer_times_out() {
        let (link, mut a, mut b) = pair(NetConfig::default(), 1, 3);
        run(&link, &mut a, &mut b, 30);
        let mut pads = Pads::new(1);
        // Only `a` keeps running; `b` has gone quiet but the connection stays open.
        let mut last = Update::Stalled;
        for _ in 0..PEER_TIMEOUT_FRAMES + 40 {
            last = a.update(|| pads.sample(0));
            link.advance();
            if matches!(last, Update::Ended(_)) {
                break;
            }
        }
        assert_eq!(last, Update::Ended(EndReason::Timeout));
        drop(b);
    }

    #[test]
    fn host_delay_is_clamped() {
        let (_link, ta, _tb) = LocalLink::pair(NetConfig::default());
        let host = Session::host(ta, 1, 200);
        assert_eq!(host.input_delay(), MAX_INPUT_DELAY as u32);
    }

    #[test]
    fn log_sink_receives_levelled_records() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let seen: Rc<RefCell<Vec<(crate::Level, String)>>> = Rc::default();
        let sink = seen.clone();
        crate::log::set_sink(Some(Box::new(move |level, target, msg| {
            sink.borrow_mut().push((level, format!("{target}: {msg}")));
        })));
        let (link, mut a, mut b) = pair(NetConfig::default(), 1, 3);
        run(&link, &mut a, &mut b, 20);
        a.quit();
        crate::log::set_sink(None);
        let seen = seen.borrow();
        assert!(seen
            .iter()
            .any(|(l, m)| *l == crate::Level::Info && m.contains("connected as")));
        assert!(seen
            .iter()
            .any(|(l, m)| *l == crate::Level::Info && m.contains("LocalQuit")));
    }
}
