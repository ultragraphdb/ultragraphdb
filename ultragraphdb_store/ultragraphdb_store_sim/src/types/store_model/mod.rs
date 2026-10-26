/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod model_reads;
mod model_recovery;
mod model_repeats;
mod model_state;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ultragraphdb_traits_store::{PersistMode, WriteBatch};

use crate::types::breach::Breach;
use crate::types::contents::Contents;
use crate::types::drop_stage::DropStage;
use crate::types::invariant::Invariant;
use crate::types::op_record::OpRecord;
use crate::types::rebuild_stage::RebuildStage;
use crate::utils::hex::hex;

use crate::types::store_model::model_recovery::partial_batch;
use crate::types::store_model::model_repeats::Answers;
use crate::types::store_model::model_state::{ModelOp, ModelState, covers};

/// The shadow model of a [`crate::SimStore`]: rebuilt from the store's log by the rules of the store's
/// specification, it says what the store must hold and what each read must return. [`StoreModel::observe`] checks
/// every logged operation; [`StoreModel::check`] compares the store's contents with the model's after a step, and
/// after a recovery finds the outcomes a crash or a power loss allows that the store's contents match. When more than
/// one matches, the model follows each, and drops one when a later record or contents rule it out; it reports a breach
/// only when none is left.
#[derive(Debug, Clone, Default)]
pub struct StoreModel {
    current: Arc<ModelState>,
    /// The state at the last sync point, which a power loss keeps.
    base: Arc<ModelState>,
    /// The writes since the last sync point, in order.
    pending: Vec<ModelOp>,
    status: Status,
    /// The open snapshots by id.
    snapshots: BTreeMap<u64, OpenSnapshot>,
    /// The temporary keyspaces by number, each holding its versions as a state's keys, and the open snapshots of
    /// them by id; none survives a recovery.
    temporaries: BTreeMap<u64, ModelState>,
    temporary_snapshots: BTreeMap<u64, Arc<ModelState>>,
    /// The read timestamps of the open snapshots, with how many are open at each.
    readers: BTreeMap<u64, usize>,
    /// What each open snapshot, of the data or of a temporary keyspace, answered the first time it was asked each
    /// read, by snapshot id: every later identical read through it must answer the same. They are what the store
    /// logged, never the model's own state, so every model the store may still match holds the same answers.
    answers: BTreeMap<u64, Answers>,
    /// Keys with a corrupted version, which the model leaves out of its comparison of the store's contents and of a
    /// temporary keyspace's reads; a drop covering a key lifts its taint.
    tainted: BTreeSet<Vec<u8>>,
    /// The newest deletion written to each key since it was last dropped.
    deletions: BTreeMap<Vec<u8>, u64>,
    /// The batches written since the last check.
    recent: Vec<ModelOp>,
    /// The drops since the last check, by their prefixes and the keys of their entries.
    drops: Vec<SeenDrop>,
    /// The outcomes the last recovery allows, waiting for the store's contents.
    recovery: Option<Vec<Outcome>>,
    /// The other models the store may still match, one per further outcome of a recovery its contents matched: each
    /// is checked against every later record and contents as this one is, and none holds others of its own.
    others: Vec<StoreModel>,
}

/// The most models the store may match at once; a recovery leaving more breaks [`Invariant::ContentsMatchModel`].
const MAX_CANDIDATES: usize = 16;

/// An open snapshot: its read timestamp, the state it reads, and the keys it leaves uncompared, those tainted when it
/// opened and those corrupted since. A snapshot keeps its taint through a drop, since it still reads the bytes it
/// opened on.
type OpenSnapshot = (u64, Arc<ModelState>, BTreeSet<Vec<u8>>);

/// A drop as the model checks it: its prefixes, of everything when `None`, and the keys of its entries.
type SeenDrop = (Option<Vec<Vec<u8>>>, BTreeSet<Vec<u8>>);

/// Whether the store serves, and what may still survive when it does not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Open,
    /// A batch failed during its persist: it may survive the recovery or not.
    Poisoned { in_flight: ModelOp },
    /// The process crashed, or its power was lost, while `in_flight` was written, if anything was.
    Down {
        power_loss: bool,
        in_flight: Option<ModelOp>,
    },
}

/// One state a recovery may end in, and the model's journal if it does.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Outcome {
    state: ModelState,
    base: Arc<ModelState>,
    pending: Vec<ModelOp>,
    /// The writes since the last sync point this outcome keeps, in order: its pending writes, or after a power loss
    /// the surviving prefix, which the outcome's sync point already holds.
    kept: Vec<ModelOp>,
    /// How many of the pending writes survived, for a power loss.
    survivors: Option<usize>,
}

impl StoreModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks and applies `log`. A recovery must be the last record of a log, so that [`StoreModel::check`] can
    /// decide its outcome before anything else happens.
    pub fn observe(&mut self, log: &[OpRecord]) -> Result<(), Breach> {
        self.each(|model| model.observe_log(log))
    }

    /// Applies `step` to this model and to each of the others, keeping those it accepts, once each: the first breach
    /// when it accepts none, leaving every model as `step` left it.
    fn each(
        &mut self,
        mut step: impl FnMut(&mut StoreModel) -> Result<(), Breach>,
    ) -> Result<(), Breach> {
        let others = std::mem::take(&mut self.others);
        let mut kept: Vec<StoreModel> = Vec::new();
        let mut refused: Vec<StoreModel> = Vec::new();
        let mut breach = None;
        for mut model in std::iter::once(std::mem::take(self)).chain(others) {
            match step(&mut model) {
                Ok(()) => {
                    let forked = std::mem::take(&mut model.others);
                    for model in std::iter::once(model).chain(forked) {
                        if !kept.iter().any(|k| k.same_as(&model)) {
                            kept.push(model);
                        }
                    }
                }
                Err(b) => {
                    breach.get_or_insert(b);
                    refused.push(model);
                }
            }
        }
        let undecided = kept.len();
        let mut models = if kept.is_empty() { refused } else { kept }.into_iter();
        *self = models.next().unwrap_or_default();
        self.others = models.collect();
        match breach {
            Some(breach) if undecided == 0 => Err(breach),
            _ if undecided > MAX_CANDIDATES => Err(Breach::new(
                Invariant::ContentsMatchModel,
                format!(
                    "the store's contents match {undecided} models, more than the {MAX_CANDIDATES} the model follows"
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Whether `other` holds the same state as this model, so that every later record and contents check alike.
    fn same_as(&self, other: &StoreModel) -> bool {
        let Self {
            current,
            base,
            pending,
            status,
            snapshots,
            temporaries,
            temporary_snapshots,
            readers,
            answers,
            tainted,
            deletions,
            recent,
            drops,
            recovery,
            others,
        } = self;
        *current == other.current
            && *base == other.base
            && *pending == other.pending
            && *status == other.status
            && *snapshots == other.snapshots
            && *temporaries == other.temporaries
            && *temporary_snapshots == other.temporary_snapshots
            && *readers == other.readers
            && *answers == other.answers
            && *tainted == other.tainted
            && *deletions == other.deletions
            && *recent == other.recent
            && *drops == other.drops
            && *recovery == other.recovery
            && others.is_empty()
            && other.others.is_empty()
    }

    fn observe_log(&mut self, log: &[OpRecord]) -> Result<(), Breach> {
        for (at, record) in log.iter().enumerate() {
            if self.recovery.is_some() {
                return Err(Breach::new(
                    Invariant::ContentsMatchModel,
                    format!("record {at} of the log follows a recovery the model has not checked"),
                ));
            }
            self.observe_one(record)?;
        }
        Ok(())
    }

    fn observe_one(&mut self, record: &OpRecord) -> Result<(), Breach> {
        match record {
            OpRecord::Applied { batch, mode } => {
                let op = self.batch_op(batch)?;
                Arc::make_mut(&mut self.current).apply(&op);
                self.recent.push(op.clone());
                self.pending.push(op);
                if *mode == PersistMode::Sync {
                    self.sync_point();
                }
            }
            OpRecord::Refused { .. } | OpRecord::Slowed { .. } => {}
            OpRecord::BatchFailed { batch } => {
                let in_flight = self.batch_op(batch)?;
                self.status = Status::Poisoned { in_flight };
            }
            OpRecord::CrashedDuringBatch { batch } => {
                let in_flight = Some(self.batch_op(batch)?);
                self.status = Status::Down {
                    power_loss: false,
                    in_flight,
                };
            }
            OpRecord::WriteWhileFailed { operation } => {
                return Err(Breach::new(
                    Invariant::RecoveryAfterFailure,
                    format!("{operation} was called on a store that had to be recovered first"),
                ));
            }
            OpRecord::Synced => self.sync_point(),
            OpRecord::Dropped {
                prefixes, entries, ..
            } => {
                let op = ModelOp::Drop {
                    prefixes: prefixes.clone(),
                    entries: entries.clone(),
                };
                Arc::make_mut(&mut self.current).apply(&op);
                self.drop_done(prefixes, entries);
                self.sync_point();
            }
            OpRecord::DropInterrupted {
                prefixes,
                entries,
                stage,
                power_loss,
                ..
            } => {
                let op = ModelOp::Drop {
                    prefixes: prefixes.clone(),
                    entries: entries.clone(),
                };
                match stage {
                    DropStage::Start => {}
                    DropStage::RecordWritten => self.pending.push(op),
                    _ => {
                        Arc::make_mut(&mut self.current).apply(&op);
                        self.drop_done(prefixes, entries);
                        self.sync_point();
                    }
                }
                self.status = Status::Down {
                    power_loss: *power_loss,
                    in_flight: None,
                };
            }
            OpRecord::RebuildRecorded { record } => {
                Arc::make_mut(&mut self.current).apply(&ModelOp::Record(record.clone()));
                self.sync_point();
            }
            OpRecord::RebuildInterrupted {
                record,
                stage,
                power_loss,
            } => {
                if *stage == RebuildStage::RecordWritten {
                    let op = ModelOp::Record(record.clone());
                    Arc::make_mut(&mut self.current).apply(&op);
                    self.pending.push(op);
                }
                self.status = Status::Down {
                    power_loss: *power_loss,
                    in_flight: None,
                };
            }
            OpRecord::TemporaryCreated { temporary } => {
                self.temporaries.insert(*temporary, ModelState::default());
            }
            OpRecord::TemporaryApplied { temporary, batch } => {
                let Some(state) = self.temporaries.get_mut(temporary) else {
                    return Err(Breach::new(
                        Invariant::ContentsMatchModel,
                        format!(
                            "a batch was written to temporary keyspace {temporary}, which does not exist"
                        ),
                    ));
                };
                state.write_temporary(batch.entries());
            }
            OpRecord::TemporaryRefused { .. } => {}
            OpRecord::TemporaryDropped { temporary } => {
                self.temporaries.remove(temporary);
            }
            OpRecord::TemporaryFailed { crashed, .. } => {
                let in_flight = ModelOp::Batch {
                    entries: Vec::new(),
                    versionless: Vec::new(),
                    applied_ts: None,
                    removed_records: Vec::new(),
                };
                self.status = if *crashed {
                    Status::Down {
                        power_loss: false,
                        in_flight: None,
                    }
                } else {
                    Status::Poisoned { in_flight }
                };
            }
            OpRecord::TemporarySnapshotOpened { id, temporary } => {
                let state = self.temporaries.get(temporary).cloned().unwrap_or_default();
                self.temporary_snapshots.insert(*id, Arc::new(state));
            }
            OpRecord::Banned { namespace } => {
                Arc::make_mut(&mut self.current).banned.insert(*namespace);
                self.sync_point();
            }
            OpRecord::DiscardSet { ts } => {
                let current = Arc::make_mut(&mut self.current);
                current.discard_ts = current.discard_ts.max(*ts);
                self.sync_point();
            }
            OpRecord::HorizonSet { ts } => {
                Arc::make_mut(&mut self.current).horizon = Some(*ts);
                self.sync_point();
            }
            OpRecord::Compacted => {
                let discard = self.discard_ts();
                Arc::make_mut(&mut self.current).compact(discard);
                self.sync_point();
            }
            OpRecord::Swept => {
                let horizon = self.tombstone_horizon();
                Arc::make_mut(&mut self.current).sweep(horizon);
                self.sync_point();
            }
            OpRecord::Corrupted { key, .. } => {
                self.tainted.insert(key.clone());
                self.snapshots.values_mut().for_each(|(_, _, tainted)| {
                    tainted.insert(key.clone());
                });
            }
            OpRecord::Crashed | OpRecord::PowerLost => {
                let lost = *record == OpRecord::PowerLost;
                self.status = match std::mem::take(&mut self.status) {
                    Status::Open => Status::Down {
                        power_loss: lost,
                        in_flight: None,
                    },
                    Status::Poisoned { in_flight } => Status::Down {
                        power_loss: lost,
                        in_flight: Some(in_flight),
                    },
                    Status::Down {
                        power_loss,
                        in_flight,
                    } => Status::Down {
                        power_loss: power_loss || lost,
                        in_flight,
                    },
                };
            }
            OpRecord::Recovered => self.recovery = Some(self.outcomes()),
            OpRecord::SnapshotOpened { .. }
            | OpRecord::SnapshotRefused { .. }
            | OpRecord::SnapshotClosed { .. }
            | OpRecord::Read { .. }
            | OpRecord::ReadVersionless { .. }
            | OpRecord::ReadMany { .. }
            | OpRecord::Scanned { .. } => self.observe_read(record)?,
        }
        self.observe_repeat(record)
    }

    /// The write `batch` makes, its entries to banned namespaces dropped, after checking that none of them lands
    /// below a deletion of its key.
    fn batch_op(&mut self, batch: &WriteBatch) -> Result<ModelOp, Breach> {
        for entry in batch.entries() {
            if let Some(&deleted) = self.deletions.get(entry.key())
                && entry.version() < deleted
            {
                return Err(Breach::new(
                    Invariant::NoWriteBelowDeletion,
                    format!(
                        "key {} written at {} below its deletion at {deleted}",
                        hex(entry.key()),
                        entry.version()
                    ),
                ));
            }
            if entry.is_deletion() {
                let newest = self.deletions.entry(entry.key().to_vec()).or_default();
                *newest = (*newest).max(entry.version());
            }
        }
        let entries = self.current.admitted(batch.entries());
        Ok(ModelOp::Batch {
            entries,
            versionless: batch.versionless().to_vec(),
            applied_ts: batch.applied_ts(),
            removed_records: batch.removed_rebuild_records().to_vec(),
        })
    }

    /// The bookkeeping of a drop that will be finished: its keys are forgotten and checked absent, and compared again
    /// in the store's contents and in the reads of snapshots opened after it.
    fn drop_done(
        &mut self,
        prefixes: &Option<Vec<Vec<u8>>>,
        entries: &[ultragraphdb_traits_store::Entry],
    ) {
        self.deletions.retain(|key, _| !covers(prefixes, key));
        self.tainted.retain(|key| !covers(prefixes, key));
        let written = entries.iter().map(|e| e.key().to_vec()).collect();
        self.drops.push((prefixes.clone(), written));
    }

    fn sync_point(&mut self) {
        self.base = self.current.clone();
        self.pending.clear();
    }

    /// The discard timestamp in effect.
    fn discard_ts(&self) -> u64 {
        let oldest = self.readers.keys().next().copied().unwrap_or(u64::MAX);
        self.current.discard_ts.min(oldest)
    }

    fn tombstone_horizon(&self) -> u64 {
        let discard = self.discard_ts();
        self.current.horizon.unwrap_or(discard).min(discard)
    }

    /// Compares the store's `contents` with the model's, or, after a recovery, finds the outcomes they match.
    pub fn check(&mut self, contents: &Contents) -> Result<(), Breach> {
        self.each(|model| model.check_contents(contents))
    }

    fn check_contents(&mut self, contents: &Contents) -> Result<(), Breach> {
        if let Some(outcomes) = self.recovery.take() {
            return self.resolve_recovery(outcomes, contents);
        }
        if let Status::Down { .. } = self.status {
            // The process is down: what it held in memory is gone, and the recovery decides what survived.
            return Ok(());
        }
        let expected = self.current.contents();
        let drops = std::mem::take(&mut self.drops);
        let recent = std::mem::take(&mut self.recent);
        let Some(difference) = contents.difference(&expected, &self.tainted) else {
            return Ok(());
        };
        if contents.applied_ts() != expected.applied_ts() {
            return Err(Breach::new(Invariant::AppliedTsIsHighest, difference));
        }
        let dropped = contents.keys().keys().find(|key| {
            !self.tainted.contains(*key)
                && expected.versions(key).is_empty()
                && drops
                    .iter()
                    .any(|(prefixes, written)| covers(prefixes, key) && !written.contains(*key))
        });
        if let Some(detail) = partial_batch(&recent, contents) {
            return Err(Breach::new(Invariant::BatchWhole, detail));
        }
        if let Some(key) = dropped {
            return Err(Breach::new(
                Invariant::DroppedPrefixAbsent,
                format!("key {} survived the drop covering it", hex(key)),
            ));
        }
        Err(Breach::new(Invariant::ContentsMatchModel, difference))
    }
}
