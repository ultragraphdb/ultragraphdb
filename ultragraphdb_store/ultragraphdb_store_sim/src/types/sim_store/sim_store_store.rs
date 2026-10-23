/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::types::drop_stage::DropStage;
use crate::types::op_record::OpRecord;
use crate::types::sim_snapshot::SimSnapshot;
use crate::types::sim_store::fault_injector::BatchOutcome;
use crate::types::sim_store::record::Record;
use crate::types::sim_store::state::DropRecord;
use crate::types::sim_store::{Inner, SimStore, Status, gc};
use crate::utils::lock::lock;

impl Store for SimStore {
    type Snapshot = SimSnapshot;

    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        let mut inner = lock(self.shared());
        inner.refuse_unless_open("apply")?;
        inner.maybe_slow();
        if let Err(error) = batch.check_key_sizes() {
            inner.log.push(OpRecord::Refused {
                batch,
                error: error.clone(),
            });
            return Err(error);
        }
        let record = inner.batch_record(batch.entries().to_vec(), batch.applied_ts());
        match inner.faults.batch() {
            BatchOutcome::Failed => {
                inner.status = Status::Poisoned { failed: record };
                inner.log.push(OpRecord::BatchFailed { batch });
                Err(StoreError::WriteFailed(
                    "simulated failure during the batch's persist".into(),
                ))
            }
            BatchOutcome::Crashed => {
                inner.status = Status::Down {
                    power_loss: false,
                    in_flight: Some(record),
                };
                inner.log.push(OpRecord::CrashedDuringBatch { batch });
                Err(StoreError::WriteFailed(
                    "simulated process crash while the batch was written".into(),
                ))
            }
            BatchOutcome::Written => {
                inner.write(record);
                if mode == PersistMode::Sync {
                    inner.sync();
                }
                inner.log.push(OpRecord::Applied { batch, mode });
                Ok(())
            }
        }
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        let mut inner = lock(self.shared());
        if matches!(inner.status, Status::Down { .. }) {
            let error = StoreError::Poisoned();
            inner.log.push(OpRecord::SnapshotRefused {
                read_ts,
                error: error.clone(),
            });
            return Err(error);
        }
        inner.maybe_slow();
        if read_ts < inner.discard_ts() {
            let error = StoreError::TsTooOld();
            inner.log.push(OpRecord::SnapshotRefused {
                read_ts,
                error: error.clone(),
            });
            return Err(error);
        }
        let id = inner.next_snapshot;
        inner.next_snapshot += 1;
        *inner.readers.entry(read_ts).or_default() += 1;
        inner.log.push(OpRecord::SnapshotOpened { id, read_ts });
        let state = inner.current.clone();
        let generation = inner.generation;
        drop(inner);
        Ok(SimSnapshot::new(
            self.shared().clone(),
            state,
            read_ts,
            id,
            generation,
        ))
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        run_drop(
            &mut lock(self.shared()),
            ts,
            Some(prefixes.to_vec()),
            entries,
        )
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
        run_drop(&mut lock(self.shared()), 0, None, entries)
    }

    fn ban_namespace(&self, namespace: u64) -> Result<(), StoreError> {
        meta_write(
            &mut lock(self.shared()),
            "ban_namespace",
            Record::Ban(namespace),
            OpRecord::Banned { namespace },
        )
    }

    fn set_discard_ts(&self, ts: u64) -> Result<(), StoreError> {
        meta_write(
            &mut lock(self.shared()),
            "set_discard_ts",
            Record::Discard(ts),
            OpRecord::DiscardSet { ts },
        )
    }

    fn discard_ts(&self) -> u64 {
        lock(self.shared()).discard_ts()
    }

    fn set_tombstone_horizon(&self, ts: u64) -> Result<(), StoreError> {
        meta_write(
            &mut lock(self.shared()),
            "set_tombstone_horizon",
            Record::Horizon(ts),
            OpRecord::HorizonSet { ts },
        )
    }

    fn tombstone_horizon(&self) -> u64 {
        lock(self.shared()).tombstone_horizon()
    }

    fn applied_ts(&self) -> u64 {
        lock(self.shared()).current.applied_ts
    }

    fn compact(&self) -> Result<(), StoreError> {
        let mut inner = lock(self.shared());
        inner.refuse_unless_open("compact")?;
        inner.maybe_slow();
        let discard = inner.discard_ts();
        let mut state = (*inner.current).clone();
        gc::compact(&mut state, &inner.corrupted, discard);
        inner.current = Arc::new(state);
        inner.sync();
        inner.log.push(OpRecord::Compacted);
        Ok(())
    }

    fn sweep_deletions(&self) -> Result<(), StoreError> {
        let mut inner = lock(self.shared());
        inner.refuse_unless_open("sweep_deletions")?;
        inner.maybe_slow();
        let horizon = inner.tombstone_horizon();
        let mut state = (*inner.current).clone();
        gc::sweep(&mut state, &inner.corrupted, horizon);
        inner.current = Arc::new(state);
        inner.sync();
        inner.log.push(OpRecord::Swept);
        Ok(())
    }

    fn sync(&self) -> Result<(), StoreError> {
        let mut inner = lock(self.shared());
        inner.refuse_unless_open("sync")?;
        inner.maybe_slow();
        inner.sync();
        inner.log.push(OpRecord::Synced);
        Ok(())
    }
}

/// A metadata entry written and synced, so that it is durable when the call returns.
fn meta_write(
    inner: &mut Inner,
    operation: &'static str,
    record: Record,
    logged: OpRecord,
) -> Result<(), StoreError> {
    inner.refuse_unless_open(operation)?;
    inner.maybe_slow();
    inner.write(record);
    inner.sync();
    inner.log.push(logged);
    Ok(())
}

/// A drop through its record: written and synced, the data removed, the entries written and synced, the record
/// deleted. A crash or power loss the fault stream draws for the drop stops it at the stage drawn with it.
fn run_drop(
    inner: &mut Inner,
    ts: u64,
    prefixes: Option<Vec<Vec<u8>>>,
    entries: Vec<Entry>,
) -> Result<(), StoreError> {
    inner.refuse_unless_open("drop")?;
    inner.maybe_slow();
    let mut check = WriteBatch::new();
    entries.iter().cloned().for_each(|e| check.push(e));
    if let Err(error) = check.check_key_sizes() {
        inner.log.push(OpRecord::Refused {
            batch: check,
            error: error.clone(),
        });
        return Err(error);
    }
    let record = DropRecord {
        ts,
        prefixes,
        entries,
    };
    let interruption = inner.faults.drop_interruption();
    let stop = |inner: &mut Inner, stage: DropStage| -> Result<(), StoreError> {
        match interruption {
            Some((at, power_loss)) if at == stage => {
                inner.status = Status::Down {
                    power_loss,
                    in_flight: None,
                };
                inner.log.push(OpRecord::DropInterrupted {
                    ts: record.ts,
                    prefixes: record.prefixes.clone(),
                    entries: record.entries.clone(),
                    stage,
                    power_loss,
                });
                Err(StoreError::WriteFailed(format!(
                    "simulated {} during a drop",
                    if power_loss {
                        "power loss"
                    } else {
                        "process crash"
                    }
                )))
            }
            _ => Ok(()),
        }
    };
    stop(inner, DropStage::Start)?;
    inner.write(Record::StartDrop(record.clone()));
    stop(inner, DropStage::RecordWritten)?;
    inner.sync();
    stop(inner, DropStage::RecordSynced)?;
    inner.remove_dropped(&record);
    stop(inner, DropStage::DataRemoved)?;
    let batch = inner.batch_record(record.entries.clone(), None);
    inner.write(batch);
    inner.sync();
    stop(inner, DropStage::EntriesSynced)?;
    inner.write(Record::EndDrop);
    inner.log.push(OpRecord::Dropped {
        ts: record.ts,
        prefixes: record.prefixes.clone(),
        entries: record.entries,
    });
    Ok(())
}
