/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod fjall_snapshot_debug;
mod fjall_snapshot_drop;
mod fjall_snapshot_store_snapshot;

use std::collections::BTreeSet;
use std::sync::Arc;

use fjall::{Keyspace, Snapshot};
use ultragraphdb_traits_store::key_namespace;

use crate::types::fjall_store::Shared;

/// A snapshot of a [`crate::FjallStore`]: one `Database::snapshot()` covering its three keyspaces, read at one
/// timestamp, with the banned namespaces of its opening. While it is open, the store's discard timestamp stays at or
/// below its read timestamp.
///
/// A compaction collects versions an all-versions scan of an open snapshot would report: versions older than a
/// discard-flagged version at or below the discard timestamp, which no read at or above that timestamp needs.
///
/// A snapshot of a temporary keyspace reads that keyspace at `u64::MAX`, with no banned namespace, and holds no
/// discard timestamp back.
pub struct FjallSnapshot {
    shared: Arc<Shared>,
    snapshot: Snapshot,
    keyspace: Keyspace,
    read_ts: u64,
    banned: Arc<BTreeSet<u64>>,
    holds_discard: bool,
}

impl FjallSnapshot {
    pub(crate) fn new(
        shared: Arc<Shared>,
        snapshot: Snapshot,
        read_ts: u64,
        banned: Arc<BTreeSet<u64>>,
    ) -> Self {
        let keyspace = shared.data.clone();
        Self {
            shared,
            snapshot,
            keyspace,
            read_ts,
            banned,
            holds_discard: true,
        }
    }

    /// The snapshot of temporary keyspace `keyspace`.
    pub(crate) fn temporary(shared: Arc<Shared>, snapshot: Snapshot, keyspace: Keyspace) -> Self {
        Self {
            shared,
            snapshot,
            keyspace,
            read_ts: u64::MAX,
            banned: Arc::default(),
            holds_discard: false,
        }
    }

    fn banned(&self, key: &[u8]) -> bool {
        key_namespace(key).is_some_and(|ns| self.banned.contains(&ns))
    }
}
