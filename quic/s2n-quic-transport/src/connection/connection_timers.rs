//! Manages all timers inside a Connection

use crate::{
    connection::InternalConnectionId,
    timer::{TimerEntry, VirtualTimer},
};
use s2n_quic_core::time::Timestamp;

/// Holds a timer entry for a single connection
///
/// On a call to [`update()`] the single per-connection timer
/// instance will be updated if changed.
pub type ConnectionTimerEntry = TimerEntry<InternalConnectionId>;

/// Stores connection-level timer state
#[derive(Debug, Default)]
pub struct ConnectionTimers {
    /// The timer which is used for closing/draining
    pub close_timer: VirtualTimer,
    /// The timer which is used to check peer idle times
    pub peer_idle_timer: VirtualTimer,
    /// The timer which is used to send packets to the peer before the idle
    /// timeout expires
    pub local_idle_timer: VirtualTimer,
}

impl ConnectionTimers {
    /// Returns an iterator of the currently armed timer timestamps
    pub fn iter(&self) -> impl Iterator<Item = &Timestamp> {
        self.close_timer
            .iter()
            .chain(self.local_idle_timer.iter())
            .chain(self.peer_idle_timer.iter())
    }
}