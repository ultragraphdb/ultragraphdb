/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_state::MetadataState;
use crate::types::principal::Principal;

impl MetadataState {
    /// The principal named `name`, `None` when none is.
    #[must_use]
    pub fn principal(&self, name: &str) -> Option<&Principal> {
        self.principals.get(name)
    }

    /// The principal whose identity is `id`, `None` when none is.
    #[must_use]
    pub fn principal_by_id(&self, id: u64) -> Option<&Principal> {
        self.principals.values().find(|p| p.id() == id)
    }

    /// The principals held. A cluster holding none refuses every connection.
    #[must_use]
    pub fn principals(&self) -> usize {
        self.principals.len()
    }

    /// The principal the session `session` acts for: the frontend's owner check.
    ///
    /// # Errors
    ///
    /// [`ClusterError::session_evicted`] when the registry does not hold `session`.
    pub fn session_principal(&self, session: u128) -> Result<u64, ClusterError> {
        self.owners
            .get(&session)
            .copied()
            .ok_or_else(|| ClusterError::session_evicted(session))
    }

    /// The sessions the registry holds.
    #[must_use]
    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }

    /// The sessions evicted that no learned close has carried yet, oldest first, at most the most one close carries:
    /// the evictions the sequencer's next close carries.
    #[must_use]
    pub fn pending_evictions(&self) -> Vec<u128> {
        self.evictions
            .iter()
            .take(MetadataCommand::EVICTIONS_MAX)
            .copied()
            .collect()
    }
}
