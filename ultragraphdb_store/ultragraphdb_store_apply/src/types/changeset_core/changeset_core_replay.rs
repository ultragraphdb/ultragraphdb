/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay (7.9): what recovery hands the applier for each logged round above the store's applied version. A logged
//! record's writes become an execution writing exactly them, each entry with the metadata its keyspace gives every
//! entry of it; a logged drop drops its predicate at its version.

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_posting::{BIT_DELTA_POSTING, CachedList, Writes};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Entry, ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::record_writes::RecordWrites;

/// The keyspace of `key`, `None` for a key of none.
fn keyspace(key: &[u8]) -> Option<Keyspace> {
    key.first()
        .and_then(|b| Keyspace::from_byte(*b))
        .map(|(k, _)| k)
}

/// The entry of `key` written at its record's version with `value`, `meta` and `discard`; its removal for none.
fn entry(key: &[u8], value: Option<&[u8]>, put: impl Fn(Vec<u8>, Vec<u8>) -> Entry) -> Entry {
    match value {
        Some(value) => put(key.to_vec(), value.to_vec()),
        None => Entry::deletion(key.to_vec(), 0),
    }
}

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The execution of `namespace` writing `record`'s writes, as [`crate::ChangesetApplier::prepare_replay`] makes
    /// it: at the watermark, reading nothing.
    pub fn prepare_replay(
        &self,
        namespace: u64,
        record: RecordWrites<'_>,
    ) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let mut deltas = Vec::new();
        let mut entries = Vec::new();
        for (key, value) in record.lists {
            if keyspace(key) == Some(Keyspace::Statistics) {
                entries.push(Entry::put(
                    key.to_vec(),
                    0,
                    BIT_DELTA_POSTING,
                    value.to_vec(),
                ));
            } else {
                deltas.push((key.to_vec(), PostingList::decode(key, value)?));
            }
        }
        entries.extend(
            record
                .statements
                .iter()
                .map(|(k, v)| entry(k, *v, |key, value| Entry::put(key, 0, 0, value))),
        );
        let versionless = record
            .dictionary
            .iter()
            .chain(record.versionless.iter())
            .map(|(key, value)| match value {
                Some(value) => VersionlessEntry::put(key.to_vec(), value.to_vec()),
                None => VersionlessEntry::removal(key.to_vec()),
            })
            .collect();
        // A record carries no catalog change: the catalog lives in the metadata group, so nothing advances the lists'
        // epoch.
        let writes = Writes::replayed(deltas, entries, versionless, false);
        let catalog = (*self.catalog(namespace)?).clone();
        Ok(Executed {
            writes,
            ..Executed::empty(namespace, read_ts, catalog)
        })
    }

    /// The drop of `target` in `namespace` replayed, as [`crate::ChangesetApplier::prepare_replayed_drop`] makes it.
    pub fn prepare_replayed_drop(
        &self,
        namespace: u64,
        target: DropTarget,
    ) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let catalog = (*self.catalog(namespace)?).clone();
        Ok(Executed::dropping(
            namespace,
            read_ts,
            catalog,
            Some(target),
            Vec::new(),
        ))
    }
}
