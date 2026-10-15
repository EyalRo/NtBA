//! NtBA online play.
//!
//! Peer-to-peer, lock-step: both peers run the same deterministic `ntba-core` simulation and
//! exchange only the one-byte-per-tick pad inputs (plus periodic state hashes to catch
//! desyncs). Nothing here knows about browsers or sockets; a [`Transport`] moves bytes.

pub mod log;
pub mod protocol;
pub mod session;
pub mod transport;

pub use log::Level;
pub use protocol::{DecodeError, Message, PROTOCOL_VERSION};
pub use session::{EndReason, Role, Session, Status, Update};
pub use transport::{Channel, LocalLink, LocalTransport, NetConfig, Transport};
