//! Online play glue: the C-ABI surface JavaScript uses to run an `ntba-net` session.
//!
//! JavaScript owns the actual WebRTC data channels. It copies every received message into
//! wasm memory (`ntba_net_alloc` + `ntba_net_push`) and drains everything the session wants to
//! send (`ntba_net_pop`). No imports are needed, so the module still instantiates with `{}`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ntba_net::log::{self, Level};
use ntba_net::session::{EndReason, Role, Session, Status, Update};
use ntba_net::{Channel, Transport};

use crate::with_state;

#[derive(Default)]
struct Pipe {
    inbox: VecDeque<Vec<u8>>,
    outbox: VecDeque<(Channel, Vec<u8>)>,
    open: bool,
}

/// A [`Transport`] whose far end is JavaScript.
pub struct JsTransport(Rc<RefCell<Pipe>>);

impl Transport for JsTransport {
    fn send(&mut self, channel: Channel, data: &[u8]) {
        let mut p = self.0.borrow_mut();
        if p.open {
            p.outbox.push_back((channel, data.to_vec()));
        }
    }

    fn recv(&mut self) -> Option<Vec<u8>> {
        self.0.borrow_mut().inbox.pop_front()
    }

    fn is_open(&self) -> bool {
        self.0.borrow().open
    }

    fn close(&mut self) {
        self.0.borrow_mut().open = false;
    }
}

pub struct Online {
    pub session: Session<JsTransport>,
    pipe: Rc<RefCell<Pipe>>,
    /// Buffer JavaScript fills before `ntba_net_push`.
    scratch: Vec<u8>,
    /// Message currently exposed by `ntba_net_pop`.
    current: (u32, Vec<u8>),
}

const NONE: u32 = u32::MAX;
const MAX_LOG_QUEUE: usize = 256;

thread_local! {
    static LOGS: RefCell<VecDeque<(Level, String)>> = const { RefCell::new(VecDeque::new()) };
    static LOG_CURRENT: RefCell<(u32, Vec<u8>)> = const { RefCell::new((0, Vec::new())) };
}

fn install_log_sink() {
    log::set_sink(Some(Box::new(|level, target, msg| {
        LOGS.with(|q| {
            let mut q = q.borrow_mut();
            if q.len() >= MAX_LOG_QUEUE {
                q.pop_front();
            }
            q.push_back((level, format!("{target}: {msg}")));
        });
    })));
}

/// Begin an online match. `role`: 0 = host (plays home, owns `seed` and `delay`), 1 = guest.
/// The data channels need not be open yet; call `ntba_net_set_open(1)` when they are.
#[no_mangle]
pub extern "C" fn ntba_net_start(role: u32, seed: u32, delay: u32) {
    install_log_sink();
    let pipe = Rc::new(RefCell::new(Pipe::default()));
    // The session sends Hello immediately, so the pipe must accept writes from the start.
    pipe.borrow_mut().open = true;
    let transport = JsTransport(pipe.clone());
    let session = if role == 0 {
        Session::host(transport, seed, delay.min(u8::MAX as u32) as u8)
    } else {
        Session::guest(transport)
    };
    with_state(|s| {
        s.online = Some(Online {
            session,
            pipe,
            scratch: Vec::new(),
            current: (0, Vec::new()),
        })
    });
}

/// Leave the online match (telling the peer) and go back to the solo game.
#[no_mangle]
pub extern "C" fn ntba_net_stop() {
    with_state(|s| {
        if let Some(o) = s.online.as_mut() {
            o.session.quit();
        }
    });
}

/// Drop the online match completely (after the final messages have been drained).
#[no_mangle]
pub extern "C" fn ntba_net_discard() {
    with_state(|s| s.online = None);
    log::set_sink(None);
}

/// Tell the session whether the data channels are usable.
#[no_mangle]
pub extern "C" fn ntba_net_set_open(open: u32) {
    with_state(|s| {
        if let Some(o) = s.online.as_mut() {
            o.pipe.borrow_mut().open = open != 0;
        }
    });
}

/// Reserve `len` bytes JavaScript can write a received message into; returns the address.
#[no_mangle]
pub extern "C" fn ntba_net_alloc(len: u32) -> *mut u8 {
    with_state(|s| match s.online.as_mut() {
        Some(o) => {
            o.scratch.clear();
            o.scratch.resize(len as usize, 0);
            o.scratch.as_mut_ptr()
        }
        None => std::ptr::null_mut(),
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Hand the bytes written into the `ntba_net_alloc` buffer to the session.
#[no_mangle]
pub extern "C" fn ntba_net_push() {
    with_state(|s| {
        if let Some(o) = s.online.as_mut() {
            let data = std::mem::take(&mut o.scratch);
            o.pipe.borrow_mut().inbox.push_back(data);
        }
    });
}

/// Move the next outgoing message into view. Returns its length, or `u32::MAX` if none; read it
/// via `ntba_net_out_ptr` and `ntba_net_out_channel` (0 = reliable, 1 = unreliable).
#[no_mangle]
pub extern "C" fn ntba_net_pop() -> u32 {
    with_state(|s| {
        let o = s.online.as_mut()?;
        let (channel, data) = o.pipe.borrow_mut().outbox.pop_front()?;
        let len = data.len() as u32;
        o.current = (u32::from(channel == Channel::Unreliable), data);
        Some(len)
    })
    .flatten()
    .unwrap_or(NONE)
}

#[no_mangle]
pub extern "C" fn ntba_net_out_ptr() -> *const u8 {
    with_state(|s| s.online.as_ref().map(|o| o.current.1.as_ptr()))
        .flatten()
        .unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn ntba_net_out_channel() -> u32 {
    with_state(|s| s.online.as_ref().map_or(0, |o| o.current.0)).unwrap_or(0)
}

/// Run one online frame with the local pad; returns 0 connecting, 1 stepped, 2 stalled, 3 ended.
#[no_mangle]
pub extern "C" fn ntba_net_step(pad: u32) -> u32 {
    with_state(|s| {
        let Some(o) = s.online.as_mut() else {
            return 3;
        };
        match o.session.update(|| pad as u8) {
            Update::Connecting => 0,
            Update::Stepped => 1,
            Update::Stalled => 2,
            Update::Ended(_) => 3,
        }
    })
    .unwrap_or(3)
}

/// 0 = no online session, 1 = connecting, 2 = running, 3 = ended.
#[no_mangle]
pub extern "C" fn ntba_net_status() -> u32 {
    with_state(|s| match s.online.as_ref().map(|o| o.session.status()) {
        None => 0,
        Some(Status::Connecting) => 1,
        Some(Status::Running) => 2,
        Some(Status::Ended(_)) => 3,
    })
    .unwrap_or(0)
}

/// Why the match ended: 0 not ended, 1 peer quit, 2 local quit, 3 desync, 4 version mismatch,
/// 5 disconnected, 6 timeout, 7 protocol error.
#[no_mangle]
pub extern "C" fn ntba_net_end_reason() -> u32 {
    with_state(|s| match s.online.as_ref().map(|o| o.session.status()) {
        Some(Status::Ended(r)) => match r {
            EndReason::PeerQuit => 1,
            EndReason::LocalQuit => 2,
            EndReason::Desync { .. } => 3,
            EndReason::VersionMismatch => 4,
            EndReason::Disconnected => 5,
            EndReason::Timeout => 6,
            EndReason::ProtocolError => 7,
        },
        _ => 0,
    })
    .unwrap_or(0)
}

/// The team this peer controls (0 home, 1 away); 0 before the handshake.
#[no_mangle]
pub extern "C" fn ntba_net_side() -> u32 {
    with_state(|s| s.online.as_ref().map_or(0, |o| o.session.side() as u32)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn ntba_net_role() -> u32 {
    with_state(|s| {
        s.online
            .as_ref()
            .map_or(0, |o| u32::from(o.session.role() == Role::Guest))
    })
    .unwrap_or(0)
}

/// Round-trip time in frames (ticks), or `u32::MAX` before the first ping is answered.
#[no_mangle]
pub extern "C" fn ntba_net_rtt() -> u32 {
    with_state(|s| s.online.as_ref().and_then(|o| o.session.rtt_frames()))
        .flatten()
        .unwrap_or(NONE)
}

#[no_mangle]
pub extern "C" fn ntba_net_tick() -> u32 {
    with_state(|s| s.online.as_ref().map_or(0, |o| o.session.tick())).unwrap_or(0)
}

/// Next queued log record: its message length, or `u32::MAX` if the queue is empty. Read the
/// bytes with `ntba_log_ptr` and the level (0 DEBUG, 1 INFO, 2 WARN, 3 ERROR) with
/// `ntba_log_level`.
#[no_mangle]
pub extern "C" fn ntba_log_next() -> u32 {
    let Some((level, msg)) = LOGS.with(|q| q.borrow_mut().pop_front()) else {
        return NONE;
    };
    let level = match level {
        Level::Debug => 0,
        Level::Info => 1,
        Level::Warn => 2,
        Level::Error => 3,
    };
    let len = msg.len() as u32;
    LOG_CURRENT.with(|c| *c.borrow_mut() = (level, msg.into_bytes()));
    len
}

#[no_mangle]
pub extern "C" fn ntba_log_ptr() -> *const u8 {
    LOG_CURRENT.with(|c| c.borrow().1.as_ptr())
}

#[no_mangle]
pub extern "C" fn ntba_log_level() -> u32 {
    LOG_CURRENT.with(|c| c.borrow().0)
}

/// Hash of the game being shown (low/high 32 bits); lets hosts and tests compare peers.
#[no_mangle]
pub extern "C" fn ntba_state_hash_lo() -> u32 {
    with_state(|s| s.shown().state_hash() as u32).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn ntba_state_hash_hi() -> u32 {
    with_state(|s| (s.shown().state_hash() >> 32) as u32).unwrap_or(0)
}
