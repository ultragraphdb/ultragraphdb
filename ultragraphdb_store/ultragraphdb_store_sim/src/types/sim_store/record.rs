/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Reverse;

use ultragraphdb_traits_store::Entry;

use crate::types::sim_store::state::{DropRecord, State, encode};

/// One record of the simulated journal: what one fjall batch or keyspace operation writes. A power loss keeps a
/// prefix of the records written since the last sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// A write batch, its entries to banned namespaces already dropped; fenced when a prefix drop writes it.
    Batch {
        entries: Vec<Entry>,
        applied_ts: Option<u64>,
        fenced: bool,
    },
    /// The drop record written to the metadata keyspace.
    StartDrop(DropRecord),
    /// Removals of stored versions, one batch of a drop.
    Remove(Vec<(Vec<u8>, u64)>),
    /// `Keyspace::clear` of the data keyspace.
    Clear,
    /// A drop of everything clearing the metadata it resets.
    ResetMeta,
    /// The drop record deleted.
    EndDrop,
    Ban(u64),
    Discard(u64),
    Horizon(u64),
}

impl Record {
    pub fn apply(&self, state: &mut State) {
        match self {
            Record::Batch {
                entries,
                applied_ts,
                fenced,
            } => {
                for entry in entries {
                    state.data.insert(
                        (entry.key().to_vec(), Reverse(entry.version())),
                        encode(entry, *fenced),
                    );
                }
                if let Some(ts) = applied_ts {
                    state.applied_ts = state.applied_ts.max(*ts);
                }
            }
            Record::StartDrop(record) => state.drop_record = Some(record.clone()),
            Record::Remove(keys) => keys.iter().for_each(|(key, version)| {
                state.data.remove(&(key.clone(), Reverse(*version)));
            }),
            Record::Clear => state.data.clear(),
            Record::ResetMeta => {
                state.applied_ts = 0;
                state.banned.clear();
                state.discard_ts = 0;
                state.horizon = None;
            }
            Record::EndDrop => state.drop_record = None,
            Record::Ban(namespace) => {
                state.banned.insert(*namespace);
            }
            Record::Discard(ts) => state.discard_ts = state.discard_ts.max(*ts),
            Record::Horizon(ts) => state.horizon = Some(*ts),
        }
    }
}
