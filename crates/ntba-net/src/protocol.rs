//! Versioned binary wire format.
//!
//! Every message is `tag: u8` followed by fixed-layout little-endian fields. Decoding is
//! strict: unknown tags, short or over-long bodies and out-of-range values are errors, never
//! panics, because the bytes come from another machine.

/// Bump when the wire format changes incompatibly.
pub const PROTOCOL_VERSION: u16 = 1;

/// Longest run of input bytes in one `Inputs` message (a bit over two seconds of play).
pub const MAX_INPUTS_PER_MESSAGE: usize = 128;

/// Why a peer is leaving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByeReason {
    /// The player quit or closed the page.
    Quit,
    /// The two peers' simulations diverged.
    Desync,
    /// The peers run different protocol or simulation builds.
    VersionMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// First message each way after the channels open (reliable).
    ///
    /// `build` identifies the simulation: it is the determinism hash from `ntba-core`, so two
    /// peers whose game rules differ in any way refuse to play. `seed` is only meaningful from
    /// the host. `side` is the team the *receiver* controls (0 = home, 1 = away).
    Hello {
        protocol: u16,
        build: u64,
        seed: u32,
        side: u8,
        input_delay: u8,
    },
    /// The sender's pad bytes for ticks `first_tick..first_tick + pads.len()` (unreliable,
    /// repeated until acknowledged). `ack` is the next remote tick the sender still needs.
    Inputs {
        ack: u32,
        first_tick: u32,
        pads: Vec<u8>,
    },
    /// State hash after simulating `tick` ticks (reliable).
    Hash {
        tick: u32,
        hash: u64,
    },
    Ping {
        id: u32,
    },
    Pong {
        id: u32,
    },
    Bye {
        reason: ByeReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Empty,
    UnknownTag(u8),
    Truncated,
    TrailingBytes,
    TooManyInputs,
    BadValue,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecodeError {}

const TAG_HELLO: u8 = 1;
const TAG_INPUTS: u8 = 2;
const TAG_HASH: u8 = 3;
const TAG_PING: u8 = 4;
const TAG_PONG: u8 = 5;
const TAG_BYE: u8 = 6;

impl Message {
    /// Messages that must arrive, and in order, go on the reliable channel.
    pub fn is_reliable(&self) -> bool {
        !matches!(
            self,
            Message::Inputs { .. } | Message::Ping { .. } | Message::Pong { .. }
        )
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24);
        match self {
            Message::Hello {
                protocol,
                build,
                seed,
                side,
                input_delay,
            } => {
                out.push(TAG_HELLO);
                out.extend_from_slice(&protocol.to_le_bytes());
                out.extend_from_slice(&build.to_le_bytes());
                out.extend_from_slice(&seed.to_le_bytes());
                out.push(*side);
                out.push(*input_delay);
            }
            Message::Inputs {
                ack,
                first_tick,
                pads,
            } => {
                debug_assert!(pads.len() <= MAX_INPUTS_PER_MESSAGE);
                out.push(TAG_INPUTS);
                out.extend_from_slice(&ack.to_le_bytes());
                out.extend_from_slice(&first_tick.to_le_bytes());
                out.push(pads.len() as u8);
                out.extend_from_slice(pads);
            }
            Message::Hash { tick, hash } => {
                out.push(TAG_HASH);
                out.extend_from_slice(&tick.to_le_bytes());
                out.extend_from_slice(&hash.to_le_bytes());
            }
            Message::Ping { id } => {
                out.push(TAG_PING);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Message::Pong { id } => {
                out.push(TAG_PONG);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Message::Bye { reason } => {
                out.push(TAG_BYE);
                out.push(match reason {
                    ByeReason::Quit => 0,
                    ByeReason::Desync => 1,
                    ByeReason::VersionMismatch => 2,
                });
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Message, DecodeError> {
        let (&tag, body) = bytes.split_first().ok_or(DecodeError::Empty)?;
        let mut r = Reader(body);
        let msg = match tag {
            TAG_HELLO => {
                let protocol = r.u16()?;
                let build = r.u64()?;
                let seed = r.u32()?;
                let side = r.u8()?;
                let input_delay = r.u8()?;
                if side > 1 {
                    return Err(DecodeError::BadValue);
                }
                Message::Hello {
                    protocol,
                    build,
                    seed,
                    side,
                    input_delay,
                }
            }
            TAG_INPUTS => {
                let ack = r.u32()?;
                let first_tick = r.u32()?;
                let n = r.u8()? as usize;
                if n > MAX_INPUTS_PER_MESSAGE {
                    return Err(DecodeError::TooManyInputs);
                }
                Message::Inputs {
                    ack,
                    first_tick,
                    pads: r.take(n)?.to_vec(),
                }
            }
            TAG_HASH => Message::Hash {
                tick: r.u32()?,
                hash: r.u64()?,
            },
            TAG_PING => Message::Ping { id: r.u32()? },
            TAG_PONG => Message::Pong { id: r.u32()? },
            TAG_BYE => Message::Bye {
                reason: match r.u8()? {
                    0 => ByeReason::Quit,
                    1 => ByeReason::Desync,
                    2 => ByeReason::VersionMismatch,
                    _ => return Err(DecodeError::BadValue),
                },
            },
            other => return Err(DecodeError::UnknownTag(other)),
        };
        if !r.0.is_empty() {
            return Err(DecodeError::TrailingBytes);
        }
        Ok(msg)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.0.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<Message> {
        vec![
            Message::Hello {
                protocol: PROTOCOL_VERSION,
                build: 0xdead_beef_0123_4567,
                seed: 42,
                side: 1,
                input_delay: 4,
            },
            Message::Inputs {
                ack: 10,
                first_tick: 7,
                pads: vec![1, 2, 3, 255],
            },
            Message::Inputs {
                ack: 0,
                first_tick: 0,
                pads: vec![],
            },
            Message::Hash {
                tick: 600,
                hash: u64::MAX,
            },
            Message::Ping { id: 9 },
            Message::Pong { id: 9 },
            Message::Bye {
                reason: ByeReason::Desync,
            },
        ]
    }

    #[test]
    fn every_message_round_trips() {
        for m in samples() {
            assert_eq!(Message::decode(&m.encode()), Ok(m));
        }
    }

    #[test]
    fn truncated_messages_are_rejected_at_every_length() {
        for m in samples() {
            let bytes = m.encode();
            for n in 0..bytes.len() {
                assert!(Message::decode(&bytes[..n]).is_err(), "{m:?} cut to {n}");
            }
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        for m in samples() {
            let mut bytes = m.encode();
            bytes.push(0);
            assert_eq!(Message::decode(&bytes), Err(DecodeError::TrailingBytes));
        }
    }

    #[test]
    fn garbage_is_rejected_without_panicking() {
        assert_eq!(Message::decode(&[]), Err(DecodeError::Empty));
        assert_eq!(Message::decode(&[0]), Err(DecodeError::UnknownTag(0)));
        assert_eq!(
            Message::decode(&[99, 1, 2]),
            Err(DecodeError::UnknownTag(99))
        );
        // Bad side, bad bye reason, oversized input run.
        let mut hello = samples()[0].encode();
        hello[15] = 2;
        assert_eq!(Message::decode(&hello), Err(DecodeError::BadValue));
        assert_eq!(Message::decode(&[TAG_BYE, 9]), Err(DecodeError::BadValue));
        let mut inputs = vec![TAG_INPUTS, 0, 0, 0, 0, 0, 0, 0, 0, 200];
        inputs.extend(vec![0; 200]);
        assert_eq!(Message::decode(&inputs), Err(DecodeError::TooManyInputs));
        // Cheap fuzz: no panic on any short pseudo-random buffer.
        let mut x = 0x1234_5678u32;
        for len in 0..64 {
            let buf: Vec<u8> = (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect();
            let _ = Message::decode(&buf);
        }
    }

    #[test]
    fn only_inputs_and_pings_are_unreliable() {
        for m in samples() {
            let unreliable = matches!(
                m,
                Message::Inputs { .. } | Message::Ping { .. } | Message::Pong { .. }
            );
            assert_eq!(m.is_reliable(), !unreliable);
        }
    }
}
