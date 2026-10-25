/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{RebuildRecord, WriteBatch};

use crate::types::op_record::OpRecord;
use crate::types::rebuild_stage::RebuildStage;
use crate::types::sim_store::Inner;
use crate::types::sim_store::record::Record;

impl Inner {
    /// Writes `record` and syncs it. The fault stream draws here where the rebuild stops, if it does: at once for its
    /// first three stages, later for the stages the store reaches afterwards.
    pub fn write_rebuild_record(&mut self, record: &RebuildRecord) -> Result<(), StoreError> {
        self.refuse_unless_open("write_rebuild_record")?;
        self.maybe_slow();
        let stop = self.faults.rebuild_interruption();
        let stops_at = |stage: RebuildStage| stop.filter(|(at, _)| *at == stage).map(|(_, p)| p);
        if let Some(power_loss) = stops_at(RebuildStage::Start) {
            return Err(self.stop_rebuild(record.clone(), RebuildStage::Start, power_loss));
        }
        self.write(Record::StartRebuild(record.clone()));
        if let Some(power_loss) = stops_at(RebuildStage::RecordWritten) {
            return Err(self.stop_rebuild(record.clone(), RebuildStage::RecordWritten, power_loss));
        }
        self.sync();
        self.log.push(OpRecord::RebuildRecorded {
            record: record.clone(),
        });
        if let Some(power_loss) = stops_at(RebuildStage::RecordSynced) {
            return Err(self.stop_rebuild(record.clone(), RebuildStage::RecordSynced, power_loss));
        }
        self.rebuild_stop = stop.map(|(stage, power_loss)| (record.clone(), stage, power_loss));
        Ok(())
    }

    /// Whether `batch` removes the record of the rebuild drawn to stop; that stop is taken when it does, and returned
    /// when it is the record's removal.
    pub fn removal_stops(&mut self, batch: &WriteBatch) -> Option<(RebuildRecord, bool)> {
        let removes = self
            .rebuild_stop
            .as_ref()
            .is_some_and(|(r, _, _)| batch.removed_rebuild_records().contains(&r.id()));
        if !removes {
            return None;
        }
        self.rebuild_stop
            .take()
            .filter(|(_, stage, _)| *stage == RebuildStage::RecordRemoved)
            .map(|(record, _, power_loss)| (record, power_loss))
    }

    /// The rebuild records the process sees.
    pub fn rebuild_records(&self) -> Vec<RebuildRecord> {
        self.current
            .rebuild_records
            .iter()
            .map(|(id, prefixes)| RebuildRecord::new(*id, prefixes.clone()))
            .collect()
    }
}
