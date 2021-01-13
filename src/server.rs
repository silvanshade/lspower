#![allow(dead_code)]

use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Atomic value which represents the current state of the server.
pub(crate) struct State(AtomicUsize);

impl State {
    pub(crate) const fn new() -> Self {
        State(AtomicUsize::new(StateKind::Uninitialized as usize))
    }

    pub(crate) fn set(&self, state: StateKind) {
        self.0.store(state as usize, Ordering::SeqCst);
    }

    pub(crate) fn get(&self) -> StateKind {
        match self.0.load(Ordering::SeqCst) {
            0 => StateKind::Uninitialized,
            1 => StateKind::Initializing,
            2 => StateKind::Initialized,
            3 => StateKind::ShutDown,
            4 => StateKind::Exited,
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.get().fmt(f)
    }
}

/// A list of possible states the language server can be in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StateKind {
    /// Server has not received an `initialize` request.
    Uninitialized = 0,
    /// Server received an `initialize` request, but has not yet responded.
    Initializing = 1,
    /// Server received and responded success to an `initialize` request.
    Initialized = 2,
    /// Server received a `shutdown` request.
    ShutDown = 3,
    /// Server received an `exit` notification.
    Exited = 4,
}
