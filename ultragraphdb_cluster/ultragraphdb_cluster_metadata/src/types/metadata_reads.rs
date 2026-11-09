/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::metadata_state::MetadataState;

/// What a [`MetadataGroup`](crate::MetadataGroup) answers reads from, apart from its log, so they are answered while the
/// log syncs ([`MetadataGroup::syncing`](crate::MetadataGroup::syncing)): the state of every durable command, and the
/// head state of every command appended, which only says whether the registry still holds a session.
#[derive(Debug, Clone, Copy)]
pub struct MetadataReads<'a> {
    state: &'a MetadataState,
    head: &'a MetadataState,
}

impl<'a> MetadataReads<'a> {
    pub(crate) fn new(state: &'a MetadataState, head: &'a MetadataState) -> Self {
        Self { state, head }
    }

    /// The state of every durable command.
    #[must_use]
    pub fn state(&self) -> &'a MetadataState {
        self.state
    }

    /// Whether the registry holds `session` after every command appended, as
    /// [`MetadataGroup::holds_session`](crate::MetadataGroup::holds_session) says.
    #[must_use]
    pub fn holds_session(&self, session: u128) -> bool {
        self.head.session_principal(session).is_ok()
    }
}
