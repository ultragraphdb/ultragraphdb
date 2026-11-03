/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod metadata_snapshot_codec;

use ultragraphdb_traits_log::Position;

use crate::types::metadata_state::MetadataState;

/// The metadata group's state as of a log position, from which recovery replays only the rounds after it: catalog
/// versions, default-entry records, high-water marks, the placement map, principals and the session registry. The host
/// stores its encoding ([`MetadataSnapshot::encode`]) durably before it trims the log through its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataSnapshot {
    position: Position,
    state: MetadataState,
}

impl MetadataSnapshot {
    /// The state `state` after every round through `position`.
    #[must_use]
    pub fn new(position: Position, state: MetadataState) -> Self {
        Self { position, state }
    }

    /// The log position the snapshot was taken at.
    #[must_use]
    pub fn position(&self) -> Position {
        self.position
    }

    /// The state through that position.
    #[must_use]
    pub fn state(&self) -> &MetadataState {
        &self.state
    }

    /// The state, the snapshot consumed.
    #[must_use]
    pub fn into_state(self) -> MetadataState {
        self.state
    }
}
