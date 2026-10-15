//! Minimal structured logging for the net layer.
//!
//! The library has no logging dependency and no I/O of its own: the host installs a sink
//! (the browser forwards to `console`, native builds to stderr or a logging platform) and
//! every record carries a standard level, a target and a message.

use std::cell::RefCell;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        })
    }
}

pub type Sink = Box<dyn Fn(Level, &str, &str)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// Install (or clear, with `None`) the log sink for the current thread.
pub fn set_sink(sink: Option<Sink>) {
    SINK.with(|s| *s.borrow_mut() = sink);
}

pub fn log(level: Level, target: &str, message: &str) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(level, target, message);
        }
    });
}

#[macro_export]
macro_rules! net_log {
    ($level:expr, $target:expr, $($arg:tt)*) => {
        $crate::log::log($level, $target, &format!($($arg)*))
    };
}
