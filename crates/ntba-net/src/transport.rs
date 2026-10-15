//! Moving bytes between two peers.
//!
//! [`Transport`] is all the session needs: a reliable ordered channel and an unreliable
//! unordered one. Browsers back it with two WebRTC data channels, a native build can use any
//! datagram + stream pair, and [`LocalTransport`] connects two peers in one process for solo
//! play and for tests with simulated latency, jitter and packet loss.

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Delivered exactly once and in order.
    Reliable,
    /// May be lost, duplicated or reordered.
    Unreliable,
}

pub trait Transport {
    fn send(&mut self, channel: Channel, data: &[u8]);
    /// Next received message from either channel, if any.
    fn recv(&mut self) -> Option<Vec<u8>>;
    /// False once the connection is closed or has failed.
    fn is_open(&self) -> bool;
    fn close(&mut self);
}

/// Simulated network conditions, in ticks (1 tick = 1/60 s).
#[derive(Clone, Copy, Debug, Default)]
pub struct NetConfig {
    /// One-way delay.
    pub latency: u32,
    /// Extra random delay of `0..=jitter` ticks per packet (reorders unreliable packets).
    pub jitter: u32,
    /// Percentage (0-100) of unreliable packets dropped.
    pub loss_percent: u8,
    /// Seed for the loss/jitter generator, so test runs are repeatable.
    pub seed: u32,
}

struct Packet {
    deliver_at: u64,
    seq: u64,
    data: Vec<u8>,
}

struct Shared {
    now: u64,
    seq: u64,
    rng: u32,
    cfg: NetConfig,
    open: bool,
    inbox: [Vec<Packet>; 2],
    last_reliable: [u64; 2],
}

impl Shared {
    fn rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }
}

/// The simulated wire between two [`LocalTransport`]s. Call [`LocalLink::advance`] once per
/// tick to move time forward.
#[derive(Clone)]
pub struct LocalLink(Rc<RefCell<Shared>>);

impl LocalLink {
    pub fn pair(cfg: NetConfig) -> (LocalLink, LocalTransport, LocalTransport) {
        let shared = Rc::new(RefCell::new(Shared {
            now: 0,
            seq: 0,
            rng: cfg.seed.wrapping_mul(0x9E37_79B1) | 1,
            cfg,
            open: true,
            inbox: [Vec::new(), Vec::new()],
            last_reliable: [0; 2],
        }));
        (
            LocalLink(shared.clone()),
            LocalTransport {
                side: 0,
                shared: shared.clone(),
            },
            LocalTransport { side: 1, shared },
        )
    }

    pub fn advance(&self) {
        self.0.borrow_mut().now += 1;
    }

    /// Break the connection from outside, as a dropped network would.
    pub fn sever(&self) {
        self.0.borrow_mut().open = false;
    }
}

pub struct LocalTransport {
    side: usize,
    shared: Rc<RefCell<Shared>>,
}

impl Transport for LocalTransport {
    fn send(&mut self, channel: Channel, data: &[u8]) {
        let mut s = self.shared.borrow_mut();
        if !s.open {
            return;
        }
        if channel == Channel::Unreliable && s.rand() % 100 < s.cfg.loss_percent as u32 {
            return;
        }
        let jitter = if s.cfg.jitter == 0 {
            0
        } else {
            s.rand() % (s.cfg.jitter + 1)
        };
        let to = 1 - self.side;
        let mut deliver_at = s.now + s.cfg.latency as u64 + jitter as u64;
        if channel == Channel::Reliable {
            // Reliable messages keep their order, like a TCP-style stream would.
            deliver_at = deliver_at.max(s.last_reliable[to]);
            s.last_reliable[to] = deliver_at;
        }
        s.seq += 1;
        let packet = Packet {
            deliver_at,
            seq: s.seq,
            data: data.to_vec(),
        };
        s.inbox[to].push(packet);
    }

    fn recv(&mut self) -> Option<Vec<u8>> {
        let mut s = self.shared.borrow_mut();
        let now = s.now;
        let inbox = &mut s.inbox[self.side];
        let best = inbox
            .iter()
            .enumerate()
            .filter(|(_, p)| p.deliver_at <= now)
            .min_by_key(|(_, p)| (p.deliver_at, p.seq))
            .map(|(i, _)| i)?;
        Some(inbox.remove(best).data)
    }

    fn is_open(&self) -> bool {
        self.shared.borrow().open
    }

    fn close(&mut self) {
        self.shared.borrow_mut().open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(t: &mut LocalTransport) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| t.recv()).collect()
    }

    #[test]
    fn delivers_after_latency() {
        let (link, mut a, mut b) = LocalLink::pair(NetConfig {
            latency: 3,
            ..NetConfig::default()
        });
        a.send(Channel::Reliable, b"hi");
        for _ in 0..3 {
            assert!(b.recv().is_none());
            link.advance();
        }
        assert_eq!(b.recv().as_deref(), Some(&b"hi"[..]));
        assert!(a.recv().is_none());
    }

    #[test]
    fn reliable_stays_ordered_despite_jitter() {
        let (link, mut a, mut b) = LocalLink::pair(NetConfig {
            latency: 2,
            jitter: 9,
            seed: 5,
            ..NetConfig::default()
        });
        for i in 0..200u8 {
            a.send(Channel::Reliable, &[i]);
            link.advance();
        }
        for _ in 0..30 {
            link.advance();
        }
        let got: Vec<u8> = drain(&mut b).into_iter().map(|p| p[0]).collect();
        assert_eq!(got, (0..200u8).collect::<Vec<_>>());
    }

    #[test]
    fn unreliable_loses_and_reorders() {
        let (link, mut a, mut b) = LocalLink::pair(NetConfig {
            latency: 1,
            jitter: 6,
            loss_percent: 30,
            seed: 11,
        });
        for i in 0..200u8 {
            a.send(Channel::Unreliable, &[i]);
            link.advance();
        }
        for _ in 0..20 {
            link.advance();
        }
        let got: Vec<u8> = drain(&mut b).into_iter().map(|p| p[0]).collect();
        assert!(got.len() < 200 && got.len() > 80, "got {}", got.len());
        assert!(got.windows(2).any(|w| w[0] > w[1]), "expected reordering");
    }

    #[test]
    fn close_and_sever_end_the_connection() {
        let (link, mut a, b) = LocalLink::pair(NetConfig::default());
        assert!(a.is_open() && b.is_open());
        a.close();
        assert!(!a.is_open() && !b.is_open());
        let (link2, a2, b2) = LocalLink::pair(NetConfig::default());
        link2.sever();
        assert!(!a2.is_open() && !b2.is_open());
        drop(link);
    }
}
