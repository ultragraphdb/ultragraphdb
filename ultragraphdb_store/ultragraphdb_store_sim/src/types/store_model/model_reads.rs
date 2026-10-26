/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::Item;

use crate::types::breach::Breach;
use crate::types::invariant::Invariant;
use crate::types::op_record::OpRecord;
use crate::types::store_model::model_state::ModelState;
use crate::types::store_model::{Status, StoreModel};
use crate::utils::hex::hex;

impl StoreModel {
    /// Checks a snapshot's opening, refusal or closing, or one of its reads, against the state it reads.
    pub(super) fn observe_read(&mut self, record: &OpRecord) -> Result<(), Breach> {
        match record {
            OpRecord::SnapshotOpened { id, read_ts } => {
                if *read_ts < self.discard_ts() {
                    return Err(Breach::new(
                        Invariant::DiscardBelowOpenReads,
                        format!(
                            "a snapshot opened at {read_ts}, below the discard timestamp {}",
                            self.discard_ts()
                        ),
                    ));
                }
                *self.readers.entry(*read_ts).or_default() += 1;
                self.snapshots
                    .insert(*id, (*read_ts, self.current.clone(), self.tainted.clone()));
                Ok(())
            }
            OpRecord::SnapshotRefused { read_ts, error } => {
                let refused_rightly = match error.0 {
                    StoreErrorEnum::TsTooOld => *read_ts < self.discard_ts(),
                    StoreErrorEnum::Poisoned => matches!(self.status, Status::Down { .. }),
                    _ => false,
                };
                if refused_rightly {
                    Ok(())
                } else {
                    Err(Breach::new(
                        Invariant::DiscardBelowOpenReads,
                        format!("a snapshot at {read_ts} was refused with {error}"),
                    ))
                }
            }
            OpRecord::SnapshotClosed { id } => {
                self.temporary_snapshots.remove(id);
                if let Some((read_ts, ..)) = self.snapshots.remove(id)
                    && let Some(count) = self.readers.get_mut(&read_ts)
                {
                    *count -= 1;
                    if *count == 0 {
                        self.readers.remove(&read_ts);
                    }
                }
                Ok(())
            }
            OpRecord::Read { id, key, result } => self.check_get(*id, key, result),
            OpRecord::ReadVersionless { id, key, result } => {
                self.check_versionless(*id, key, result)
            }
            OpRecord::ReadMany { id, keys, result } => match result {
                Ok(items) => keys
                    .iter()
                    .zip(items)
                    .try_for_each(|(key, item)| self.check_get(*id, key, &Ok(item.clone()))),
                Err(error) => self.check_failure(self.tainted_for(*id), keys, error),
            },
            OpRecord::Scanned { id, scan, items } => {
                let Some((read_ts, state)) = self.reading(*id) else {
                    return Ok(());
                };
                let tainted = self.tainted_for(*id);
                let keep = |item: &&Item| !tainted.contains(item.key());
                let wanted = state.scan(scan, *read_ts);
                let wanted: Vec<&Item> = wanted.iter().filter(keep).collect();
                let (got, failure): (Vec<&Item>, Option<&StoreError>) = match items.split_last() {
                    Some((Err(error), rest)) => (
                        rest.iter().filter_map(|r| r.as_ref().ok()).collect(),
                        Some(error),
                    ),
                    _ => (items.iter().filter_map(|r| r.as_ref().ok()).collect(), None),
                };
                let got: Vec<&Item> = got.into_iter().filter(|i| keep(i)).collect();
                if let Some(error) = failure {
                    let key = error_key(error).map(<[u8]>::to_vec).unwrap_or_default();
                    self.check_failure(tainted, &[key], error)?;
                }
                let matches = match failure {
                    None => got == wanted,
                    Some(_) => wanted.starts_with(&got),
                };
                if matches {
                    Ok(())
                } else {
                    Err(Breach::new(
                        Invariant::ReadsAtOrBelowTimestamp,
                        format!(
                            "a scan at {read_ts} of {scan:?} returned {got:?} where {wanted:?} was expected"
                        ),
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    /// The read timestamp and the state snapshot `id` reads, a temporary keyspace's at `u64::MAX`.
    fn reading(&self, id: u64) -> Option<(&u64, &ModelState)> {
        const EVERY_VERSION: u64 = u64::MAX;
        self.snapshots
            .get(&id)
            .map(|(ts, state, _)| (ts, state.as_ref()))
            .or_else(|| {
                self.temporary_snapshots
                    .get(&id)
                    .map(|state| (&EVERY_VERSION, state.as_ref()))
            })
    }

    /// The keys snapshot `id` leaves uncompared: its own, or the store's for a temporary keyspace or an unknown snapshot.
    fn tainted_for(&self, id: u64) -> &BTreeSet<Vec<u8>> {
        self.snapshots
            .get(&id)
            .map_or(&self.tainted, |(_, _, tainted)| tainted)
    }

    /// Checks a point read of `key` through snapshot `id`.
    fn check_get(
        &self,
        id: u64,
        key: &[u8],
        result: &Result<Option<Item>, StoreError>,
    ) -> Result<(), Breach> {
        let Some((read_ts, state)) = self.reading(id) else {
            return Ok(());
        };
        let tainted = self.tainted_for(id);
        if tainted.contains(key) {
            return Ok(());
        }
        match result {
            Err(error) => self.check_failure(tainted, &[key.to_vec()], error),
            Ok(item) => {
                let wanted = state.get(key, *read_ts);
                if *item == wanted {
                    Ok(())
                } else {
                    Err(Breach::new(
                        Invariant::ReadsAtOrBelowTimestamp,
                        format!(
                            "a read of key {} at {read_ts} returned {item:?} where {wanted:?} was expected",
                            hex(key)
                        ),
                    ))
                }
            }
        }
    }

    /// Checks a read of `key` in the keyspace without versions, through snapshot `id` against the state it opened on,
    /// or through the store against what the process sees; a temporary keyspace's snapshot holds none. The keyspace
    /// is never corrupted, so no key is left uncompared.
    fn check_versionless(
        &self,
        id: Option<u64>,
        key: &[u8],
        result: &Result<Option<Vec<u8>>, StoreError>,
    ) -> Result<(), Breach> {
        let wanted = match id {
            Some(id) if self.temporary_snapshots.contains_key(&id) => None,
            Some(id) => match self.snapshots.get(&id) {
                Some((_, state, _)) => state.versionless.get(key).cloned(),
                None => return Ok(()),
            },
            None => {
                if matches!(self.status, Status::Down { .. }) {
                    return match result {
                        Err(e) if matches!(e.0, StoreErrorEnum::Poisoned) => Ok(()),
                        _ => Err(Breach::new(
                            Invariant::ReadsAtOrBelowTimestamp,
                            format!(
                                "a versionless read of key {} on a store that was down returned {result:?}",
                                hex(key)
                            ),
                        )),
                    };
                }
                self.current.versionless.get(key).cloned()
            }
        };
        match result {
            Err(e) if matches!(e.0, StoreErrorEnum::ReadFailed { .. }) => Ok(()),
            Ok(value) if *value == wanted => Ok(()),
            other => Err(Breach::new(
                Invariant::ReadsAtOrBelowTimestamp,
                format!(
                    "a versionless read of key {} returned {other:?} where {wanted:?} was expected",
                    hex(key)
                ),
            )),
        }
    }

    /// A read may fail as the fault stream makes it, or with `Corrupted` on a key `tainted` holds.
    fn check_failure(
        &self,
        tainted: &BTreeSet<Vec<u8>>,
        keys: &[Vec<u8>],
        error: &StoreError,
    ) -> Result<(), Breach> {
        let allowed = match &error.0 {
            StoreErrorEnum::ReadFailed { .. } => true,
            StoreErrorEnum::Corrupted { key, .. } => tainted.contains(key),
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(Breach::new(
                Invariant::ReadsAtOrBelowTimestamp,
                format!(
                    "a read of {} failed with {error}",
                    keys.iter().map(|k| hex(k)).collect::<Vec<_>>().join(", ")
                ),
            ))
        }
    }
}

/// The key a read failure names.
fn error_key(error: &StoreError) -> Option<&[u8]> {
    match &error.0 {
        StoreErrorEnum::ReadFailed { key, .. } | StoreErrorEnum::Corrupted { key, .. } => Some(key),
        _ => None,
    }
}
