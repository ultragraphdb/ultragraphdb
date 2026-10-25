/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::{Item, Scan};

use crate::types::breach::Breach;
use crate::types::invariant::Invariant;
use crate::types::op_record::OpRecord;
use crate::types::store_model::StoreModel;
use crate::utils::hex::hex;

/// The most reads whose answers a snapshot holds at once.
const MAX_ANSWERS: usize = 64;

/// What a point read of one key answered.
type PointAnswer = Result<Option<Item>, StoreError>;

/// The answers an open snapshot gave the first time it was asked each read: a point read or a key of a many-key
/// read by its key, which both read alike, and a scan by its whole spec. Each later identical read must answer the
/// same. A corruption of a key forgets the answers covering it, and the snapshot holds at most [`MAX_ANSWERS`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Answers {
    points: BTreeMap<Vec<u8>, PointAnswer>,
    scans: Vec<(Scan, Vec<Result<Item, StoreError>>)>,
}

impl Answers {
    fn full(&self) -> bool {
        self.points.len() + self.scans.len() >= MAX_ANSWERS
    }

    /// Forgets every answer a corruption of `key` may change: its point read and each scan selecting it.
    fn forget(&mut self, key: &[u8]) {
        self.points.remove(key);
        self.scans.retain(|(scan, _)| !scan.selects(key));
    }

    /// Compares `answer` with the first answer to a read of `key`, if one is held.
    fn compare_point(&self, id: u64, key: &[u8], answer: &PointAnswer) -> Result<(), Breach> {
        match self.points.get(key) {
            Some(first) if first != answer => Err(Breach::new(
                Invariant::SnapshotReadsRepeat,
                format!(
                    "snapshot {id} read key {} as {answer:?} where it read {first:?} the first time",
                    hex(key)
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Holds `answer` as the first answer to a read of `key`, unless one is held or the snapshot holds its most.
    fn hold_point(&mut self, key: &[u8], answer: PointAnswer) {
        if !self.points.contains_key(key) && !self.full() {
            self.points.insert(key.to_vec(), answer);
        }
    }

    /// Compares the items of `scan` with its first answer, or holds them as the first. Items a read failure cut short
    /// are compared as a prefix of the first answer and never held.
    fn scan(
        &mut self,
        id: u64,
        scan: &Scan,
        items: &[Result<Item, StoreError>],
    ) -> Result<(), Breach> {
        let (got, cut) = match items.split_last() {
            Some((Err(error), rest)) if injected(error) => (rest, true),
            _ => (items, false),
        };
        match self.scans.iter().find(|(held, _)| held == scan) {
            Some((_, first)) => {
                let same = if cut {
                    first.starts_with(got)
                } else {
                    first.as_slice() == got
                };
                if same {
                    Ok(())
                } else {
                    Err(Breach::new(
                        Invariant::SnapshotReadsRepeat,
                        format!(
                            "snapshot {id} scanned {scan:?} as {got:?}{} where it scanned {first:?} the first time",
                            if cut { " before a read failure," } else { "" }
                        ),
                    ))
                }
            }
            None => {
                if !cut && !self.full() {
                    self.scans.push((scan.clone(), got.to_vec()));
                }
                Ok(())
            }
        }
    }
}

impl StoreModel {
    /// Checks that a read through an open snapshot answers as the same read did the first time, and keeps the
    /// answers: a snapshot's opening starts them, its closing ends them, and a corruption forgets those covering its
    /// key in the snapshots of the data at or above its version. A read the fault stream failed is not an answer.
    pub(super) fn observe_repeat(&mut self, record: &OpRecord) -> Result<(), Breach> {
        match record {
            OpRecord::SnapshotOpened { id, .. } | OpRecord::TemporarySnapshotOpened { id, .. } => {
                self.answers.insert(*id, Answers::default());
            }
            OpRecord::SnapshotClosed { id } => {
                self.answers.remove(id);
            }
            OpRecord::Corrupted { key, version, .. } => {
                // Only a snapshot of the data at or above the version reads it: a temporary keyspace's bytes are
                // never corrupted, and a snapshot below the version never reaches it.
                let reaches = |id: &u64| {
                    self.snapshots
                        .get(id)
                        .is_some_and(|(read_ts, ..)| version <= read_ts)
                };
                self.answers
                    .iter_mut()
                    .filter(|(id, _)| reaches(id))
                    .for_each(|(_, a)| a.forget(key));
            }
            OpRecord::Read { id, key, result } => {
                if let Some(answers) = self.answers.get_mut(id)
                    && !result.as_ref().is_err_and(injected)
                {
                    answers.compare_point(*id, key, result)?;
                    answers.hold_point(key, result.clone());
                }
            }
            OpRecord::ReadMany { id, keys, result } => {
                if let Some(answers) = self.answers.get_mut(id) {
                    // Compared with the answers held before the call, not with each other: a key named twice is read
                    // twice, and a corruption drawn at its second read is logged before the call. The last read of a
                    // key follows every corruption drawn during the call, so it is the answer held.
                    let read: Vec<(&[u8], PointAnswer)> = match result {
                        Ok(items) => keys
                            .iter()
                            .zip(items)
                            .map(|(key, item)| (key.as_slice(), Ok(item.clone())))
                            .collect(),
                        Err(error) => match &error.0 {
                            StoreErrorEnum::Corrupted { key, .. } => {
                                vec![(key.as_slice(), Err(error.clone()))]
                            }
                            _ => Vec::new(),
                        },
                    };
                    read.iter()
                        .try_for_each(|(key, answer)| answers.compare_point(*id, key, answer))?;
                    read.into_iter()
                        .rev()
                        .for_each(|(key, answer)| answers.hold_point(key, answer));
                }
            }
            OpRecord::Scanned { id, scan, items } => {
                if let Some(answers) = self.answers.get_mut(id) {
                    answers.scan(*id, scan, items)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Whether `error` is a failure the fault stream injected, which is not a snapshot's answer.
fn injected(error: &StoreError) -> bool {
    matches!(error.0, StoreErrorEnum::ReadFailed { .. })
}
