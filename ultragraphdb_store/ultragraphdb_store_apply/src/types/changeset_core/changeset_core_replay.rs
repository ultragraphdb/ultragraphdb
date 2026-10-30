/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay (7.9): what recovery hands the applier for each logged round above the store's applied version. A logged
//! record's writes become an execution writing exactly them, each entry with the metadata its keyspace gives every
//! entry of it; a logged drop drops its predicate at its version; a logged build's end has its build restored from the
//! declaration its record names, so the round's apply builds it again and swaps it in, or undoes it.

use std::collections::BTreeSet;

use ultragraphdb_core_catalog::{Catalog, decode_declaration};
use ultragraphdb_core_codec::PostingList;
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_store_posting::{BIT_DELTA_POSTING, CachedList, Writes};
use ultragraphdb_store_quads::WriteTerms;
use ultragraphdb_traits_store::{Entry, ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::build_end::BuildEnd;
use crate::types::changeset_core::ChangesetCore;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::reads::Reads;
use crate::types::rebuild::Rebuild;
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

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
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
        let advance_epoch = !record.catalog.is_empty();
        entries.extend(
            record
                .catalog
                .iter()
                .map(|(k, v)| entry(k, *v, |key, value| Catalog::stored_entry(key, value, 0))),
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
        let writes = Writes::replayed(deltas, entries, versionless, advance_epoch);
        let catalog = (*self.catalog(namespace)?).clone();
        Ok(Executed::new(
            namespace,
            read_ts,
            writes,
            WriteTerms::new(namespace),
            Reads::default(),
            BTreeSet::new(),
            0,
            catalog,
            (Vec::new(), Vec::new(), false),
            None,
            BTreeSet::new(),
            Vec::new(),
        ))
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

    /// Restores the build `end` ends from the catalog entry `key` and `value` its record names, as
    /// [`crate::ChangesetApplier::restore_build`] does: whether it was restored.
    pub fn restore_build(
        &self,
        namespace: u64,
        end: BuildEnd,
        key: &[u8],
        value: &[u8],
    ) -> Result<bool, ChangesetError> {
        let (Ok((QuadKey::Catalog { uid, .. }, _)), Declaration::Predicate(recorded)) =
            (QuadKey::parse(key), decode_declaration(key, value)?)
        else {
            return Ok(false);
        };
        let Some(current) = self.catalog(namespace)?.predicate(uid).cloned() else {
            return Ok(false);
        };
        let (record, old, new) = match end {
            BuildEnd::Swap(record) => (record, current, recorded),
            BuildEnd::Undo(record) => (record, recorded, current),
        };
        let mut rebuild = Rebuild::pending(namespace, uid, old, new);
        rebuild.record = record;
        rebuild.start_ts = self.quads.lists().applied_ts();
        self.begin(rebuild);
        Ok(true)
    }
}
