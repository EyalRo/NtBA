//! Six-character save codes for resuming a playoff run.
//!
//! A code is six characters from a 32-letter alphabet with no `0/O` and no `1/I` (`L` stays, as
//! `1` and `I` are gone). The first five characters carry 25 bits: 22 bits of [`SaveState`]
//! and 3 reserved bits that must be zero. The sixth is a check character, a weighted sum of
//! the first five modulo 32 with odd weights, so **every** single-character typo is rejected.
//!
//! Decoding is case-insensitive and ignores spaces and dashes, so `"abc-d23"` style input
//! typed from a screen works.
//!
//! ```
//! use ntba_core::savecode::{decode, encode, SaveState};
//! let s = SaveState { round: 2, series_wins: [3, 1], opponent: 9, difficulty: 1, flags: 0 };
//! let code = encode(&s);
//! assert_eq!(code.len(), 6);
//! assert_eq!(decode(&code.to_lowercase()), Ok(s));
//! ```

use std::fmt;

/// The 32 characters a code is made of. No `0`, `O`, `1` or `I`.
pub const ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Number of characters in a code.
pub const CODE_LEN: usize = 6;

/// Weights of the first five characters in the check sum. Odd, so each is invertible mod 32.
const WEIGHTS: [u32; CODE_LEN - 1] = [1, 3, 5, 7, 9];

/// The run state a code stores. Every field has a documented range; [`encode`] panics outside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveState {
    /// Playoff round about to be played, `0..=3`.
    pub round: u8,
    /// Series wins of `[player, opponent]` in the current round, each `0..=4`.
    pub series_wins: [u8; 2],
    /// Index of the current opponent, `0..=15`.
    pub opponent: u8,
    /// Difficulty level, `0..=3`.
    pub difficulty: u8,
    /// Mode flags; the meaning of each bit is assigned by the playoff mode.
    pub flags: u8,
}

impl SaveState {
    /// True when every field is inside its documented range.
    pub fn is_valid(&self) -> bool {
        self.round <= 3
            && self.series_wins.iter().all(|&w| w <= 4)
            && self.opponent <= 15
            && self.difficulty <= 3
    }
}

/// Why a code could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not six characters once spaces and dashes are removed.
    BadLength,
    /// A character outside the alphabet.
    BadCharacter(char),
    /// The check character does not match: a typo.
    BadChecksum,
    /// The characters are valid but describe a state that cannot exist.
    OutOfRange,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::BadLength => write!(f, "a code has {CODE_LEN} characters"),
            DecodeError::BadCharacter(c) => write!(f, "'{c}' is not used in codes"),
            DecodeError::BadChecksum => write!(f, "that code is mistyped"),
            DecodeError::OutOfRange => write!(f, "that code is not valid"),
        }
    }
}

impl std::error::Error for DecodeError {}

fn check_char(payload: &[u32]) -> u32 {
    payload.iter().zip(WEIGHTS).map(|(c, w)| c * w).sum::<u32>() % 32
}

/// Encode a state as a six-character code.
///
/// # Panics
/// If a field of `state` is outside its documented range.
pub fn encode(state: &SaveState) -> String {
    assert!(state.is_valid(), "save state out of range: {state:?}");
    let v = state.round as u32
        | (state.series_wins[0] as u32) << 2
        | (state.series_wins[1] as u32) << 5
        | (state.opponent as u32) << 8
        | (state.difficulty as u32) << 12
        | (state.flags as u32) << 14;
    let mut chars: Vec<u32> = (0..CODE_LEN - 1).map(|i| (v >> (5 * i)) & 31).collect();
    chars.push(check_char(&chars));
    chars
        .iter()
        .map(|&c| ALPHABET[c as usize] as char)
        .collect()
}

/// Read a code back. Case-insensitive; spaces and dashes are ignored.
pub fn decode(code: &str) -> Result<SaveState, DecodeError> {
    let mut chars = Vec::with_capacity(CODE_LEN);
    for c in code.chars().filter(|c| !matches!(c, ' ' | '-')) {
        let up = c.to_ascii_uppercase();
        match ALPHABET.iter().position(|&a| a as char == up) {
            Some(i) => chars.push(i as u32),
            None => return Err(DecodeError::BadCharacter(c)),
        }
    }
    if chars.len() != CODE_LEN {
        return Err(DecodeError::BadLength);
    }
    if check_char(&chars[..CODE_LEN - 1]) != chars[CODE_LEN - 1] {
        return Err(DecodeError::BadChecksum);
    }
    let v = chars[..CODE_LEN - 1]
        .iter()
        .enumerate()
        .fold(0u32, |v, (i, &c)| v | c << (5 * i));
    let state = SaveState {
        round: (v & 3) as u8,
        series_wins: [((v >> 2) & 7) as u8, ((v >> 5) & 7) as u8],
        opponent: ((v >> 8) & 15) as u8,
        difficulty: ((v >> 12) & 3) as u8,
        flags: ((v >> 14) & 255) as u8,
    };
    if v >> 22 != 0 || !state.is_valid() {
        return Err(DecodeError::OutOfRange);
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(round: u8, w0: u8, w1: u8, opponent: u8, difficulty: u8, flags: u8) -> SaveState {
        SaveState {
            round,
            series_wins: [w0, w1],
            opponent,
            difficulty,
            flags,
        }
    }

    #[test]
    fn round_trips_every_field_range() {
        let mut n = 0;
        for round in 0..=3 {
            for w0 in 0..=4 {
                for w1 in 0..=4 {
                    for opponent in 0..=15 {
                        for difficulty in 0..=3 {
                            // Every flag value for a spread of states, edge values for the rest.
                            let step = if (round + w0 + w1 + opponent + difficulty) % 7 == 0 {
                                1
                            } else {
                                85
                            };
                            for flags in (0..=255u8).step_by(step) {
                                let s = state(round, w0, w1, opponent, difficulty, flags);
                                let code = encode(&s);
                                assert_eq!(code.len(), CODE_LEN);
                                assert_eq!(decode(&code), Ok(s), "code {code}");
                                n += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(n > 10_000);
    }

    #[test]
    fn rejects_every_single_character_substitution() {
        let samples = [
            state(0, 0, 0, 0, 0, 0),
            state(3, 4, 4, 15, 3, 255),
            state(2, 3, 1, 9, 1, 0),
            state(1, 0, 2, 4, 2, 0b1010_0101),
            state(3, 1, 4, 7, 0, 1),
        ];
        for s in samples {
            let code = encode(&s);
            for pos in 0..CODE_LEN {
                for &repl in ALPHABET {
                    let mut bytes = code.clone().into_bytes();
                    if bytes[pos] == repl {
                        continue;
                    }
                    bytes[pos] = repl;
                    let typo = String::from_utf8(bytes).unwrap();
                    assert!(decode(&typo).is_err(), "{code} -> {typo} was accepted");
                }
            }
        }
    }

    #[test]
    fn alphabet_is_32_unique_unambiguous_letters() {
        let mut seen = ALPHABET.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 32);
        assert!(ALPHABET
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        for bad in b"0O1I" {
            assert!(!ALPHABET.contains(bad), "{}", *bad as char);
        }
    }

    #[test]
    fn ignores_case_spaces_and_dashes() {
        let s = state(2, 3, 1, 9, 1, 40);
        let code = encode(&s);
        let spaced = format!("{}-{} {}", &code[..2], &code[2..4], &code[4..]).to_lowercase();
        assert_eq!(decode(&spaced), Ok(s));
    }

    #[test]
    fn reports_why_a_code_was_refused() {
        let code = encode(&state(1, 2, 3, 4, 1, 0));
        assert_eq!(decode(&code[..5]), Err(DecodeError::BadLength));
        assert_eq!(decode(&format!("{code}2")), Err(DecodeError::BadLength));
        assert_eq!(decode("0AAAAA"), Err(DecodeError::BadCharacter('0')));
        let mut bytes = code.into_bytes();
        bytes[0] = if bytes[0] == b'2' { b'3' } else { b'2' };
        assert_eq!(
            decode(std::str::from_utf8(&bytes).unwrap()),
            Err(DecodeError::BadChecksum)
        );
    }

    #[test]
    fn rejects_well_formed_codes_for_impossible_states() {
        // A valid checksum over wins = 7 (above the 0..=4 range).
        let v = 7u32 << 2;
        let mut chars: Vec<u32> = (0..CODE_LEN - 1).map(|i| (v >> (5 * i)) & 31).collect();
        chars.push(check_char(&chars));
        let code: String = chars
            .iter()
            .map(|&c| ALPHABET[c as usize] as char)
            .collect();
        assert_eq!(decode(&code), Err(DecodeError::OutOfRange));
        // The reserved top bits must be zero.
        let v = 1u32 << 22;
        let mut chars: Vec<u32> = (0..CODE_LEN - 1).map(|i| (v >> (5 * i)) & 31).collect();
        chars.push(check_char(&chars));
        let code: String = chars
            .iter()
            .map(|&c| ALPHABET[c as usize] as char)
            .collect();
        assert_eq!(decode(&code), Err(DecodeError::OutOfRange));
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn encode_refuses_an_out_of_range_state() {
        encode(&state(4, 0, 0, 0, 0, 0));
    }
}
