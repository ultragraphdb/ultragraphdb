/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, WriteBatch};

use crate::types::breach::Breach;
use crate::types::drop_stage::DropStage;
use crate::types::invariant::Invariant;
use crate::types::rebuild_stage::RebuildStage;
use crate::types::store_model::model_state::ModelOp;
use crate::types::store_model::{Status, StoreModel};

/// How the model follows each write, drop, rebuild, temporary keyspace and failure the store logs.
impl StoreModel {
    pub(super) fn observe_applied(
        &mut self,
        batch: &WriteBatch,
        mode: PersistMode,
    ) -> Result<(), Breach> {
        let op = self.batch_op(batch)?;
        Arc::make_mut(&mut self.current).apply(&op);
        self.recent.push(op.clone());
        self.pending.push(op);
        if mode == PersistMode::Sync {
            self.sync_point();
        }
        Ok(())
    }

    pub(super) fn observe_batch_failed(&mut self, batch: &WriteBatch) -> Result<(), Breach> {
        let in_flight = self.batch_op(batch)?;
        self.status = Status::Poisoned { in_flight };
        Ok(())
    }

    pub(super) fn observe_crashed_during_batch(
        &mut self,
        batch: &WriteBatch,
    ) -> Result<(), Breach> {
        let in_flight = Some(self.batch_op(batch)?);
        self.status = Status::Down {
            power_loss: false,
            in_flight,
        };
        Ok(())
    }

    pub(super) fn observe_dropped(&mut self, prefixes: &Option<Vec<Vec<u8>>>, entries: &[Entry]) {
        let op = ModelOp::Drop {
            prefixes: prefixes.clone(),
            entries: entries.to_vec(),
        };
        Arc::make_mut(&mut self.current).apply(&op);
        self.drop_done(prefixes, entries);
        self.sync_point();
    }

    pub(super) fn observe_drop_interrupted(
        &mut self,
        prefixes: &Option<Vec<Vec<u8>>>,
        entries: &[Entry],
        stage: DropStage,
        power_loss: bool,
    ) {
        let op = ModelOp::Drop {
            prefixes: prefixes.clone(),
            entries: entries.to_vec(),
        };
        match stage {
            DropStage::Start => {}
            DropStage::RecordWritten => self.pending.push(op),
            DropStage::RecordSynced | DropStage::DataRemoved | DropStage::EntriesSynced => {
                Arc::make_mut(&mut self.current).apply(&op);
                self.drop_done(prefixes, entries);
                self.sync_point();
            }
        }
        self.status = Status::Down {
            power_loss,
            in_flight: None,
        };
    }

    pub(super) fn observe_rebuild_recorded(&mut self, record: &RebuildRecord) {
        Arc::make_mut(&mut self.current).apply(&ModelOp::Record(record.clone()));
        self.sync_point();
    }

    pub(super) fn observe_rebuild_interrupted(
        &mut self,
        record: &RebuildRecord,
        stage: RebuildStage,
        power_loss: bool,
    ) {
        if stage == RebuildStage::RecordWritten {
            let op = ModelOp::Record(record.clone());
            Arc::make_mut(&mut self.current).apply(&op);
            self.pending.push(op);
        }
        self.status = Status::Down {
            power_loss,
            in_flight: None,
        };
    }

    pub(super) fn observe_temporary_applied(
        &mut self,
        temporary: u64,
        batch: &WriteBatch,
    ) -> Result<(), Breach> {
        let Some(state) = self.temporaries.get_mut(&temporary) else {
            return Err(Breach::new(
                Invariant::ContentsMatchModel,
                format!(
                    "a batch was written to temporary keyspace {temporary}, which does not exist"
                ),
            ));
        };
        state.write_temporary(batch.entries());
        Ok(())
    }

    pub(super) fn observe_temporary_failed(&mut self, crashed: bool) {
        let in_flight = ModelOp::Batch {
            entries: Vec::new(),
            versionless: Vec::new(),
            applied_ts: None,
            removed_records: Vec::new(),
        };
        self.status = if crashed {
            Status::Down {
                power_loss: false,
                in_flight: None,
            }
        } else {
            Status::Poisoned { in_flight }
        };
    }

    pub(super) fn observe_temporary_snapshot(&mut self, id: u64, temporary: u64) {
        let state = self
            .temporaries
            .get(&temporary)
            .cloned()
            .unwrap_or_default();
        self.temporary_snapshots.insert(id, Arc::new(state));
    }

    pub(super) fn observe_banned(&mut self, namespace: u64) {
        Arc::make_mut(&mut self.current).banned.insert(namespace);
        self.sync_point();
    }

    pub(super) fn observe_discard_set(&mut self, ts: u64) {
        let current = Arc::make_mut(&mut self.current);
        current.discard_ts = current.discard_ts.max(ts);
        self.sync_point();
    }

    pub(super) fn observe_horizon_set(&mut self, ts: u64) {
        Arc::make_mut(&mut self.current).horizon = Some(ts);
        self.sync_point();
    }

    pub(super) fn observe_compacted(&mut self) {
        let discard = self.discard_ts();
        Arc::make_mut(&mut self.current).compact(discard);
        self.sync_point();
    }

    pub(super) fn observe_swept(&mut self) {
        let horizon = self.tombstone_horizon();
        Arc::make_mut(&mut self.current).sweep(horizon);
        self.sync_point();
    }

    pub(super) fn observe_corrupted(&mut self, key: &[u8]) {
        self.tainted.insert(key.to_vec());
        self.snapshots.values_mut().for_each(|(_, _, tainted)| {
            tainted.insert(key.to_vec());
        });
    }

    /// A crash, or a power loss when `lost`, takes the process down with whatever it was writing.
    pub(super) fn observe_crash(&mut self, lost: bool) {
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
}
