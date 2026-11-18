//! Internal synchronous report-transaction context borrowing immutable
//! connection/policy state and prior published transfer evidence over a
//! mutable candidate [`crate::world_engine::WorldState`].

use crate::world_connection_state::WorldConnectionState;
use crate::world_engine::WorldState;
use crate::world_report::TransferTargetV4;

/// Synchronous execution context for one admitted report or staged census.
/// Borrows immutable connection/policy and prior published transfer evidence;
/// mutates only the candidate [`WorldState`].
pub(crate) struct WorldTransaction<'a> {
    pub(crate) connection: &'a WorldConnectionState,
    pub(crate) prev_reply_transfers: &'a [TransferTargetV4],
    pub(crate) world: &'a mut WorldState,
}

impl<'a> WorldTransaction<'a> {
    #[must_use]
    pub(crate) fn new(
        connection: &'a WorldConnectionState,
        prev_reply_transfers: &'a [TransferTargetV4],
        world: &'a mut WorldState,
    ) -> Self {
        Self {
            connection,
            prev_reply_transfers,
            world,
        }
    }
}
