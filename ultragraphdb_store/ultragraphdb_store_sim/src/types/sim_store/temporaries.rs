/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Reverse;
use std::sync::Arc;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Temporary, WriteBatch};

use crate::types::op_record::OpRecord;
use crate::types::rebuild_stage::RebuildStage;
use crate::types::sim_store::fault_injector::BatchOutcome;
use crate::types::sim_store::record::Record;
use crate::types::sim_store::state::{State, encode};
use crate::types::sim_store::{Inner, Status};

impl Inner {
    /// A new, empty temporary keyspace.
    pub fn create_temporary(&mut self) -> Result<Temporary, StoreError> {
        self.refuse_unless_open("create_temporary")?;
        self.maybe_slow();
        let id = self.next_temporary;
        self.next_temporary += 1;
        self.temporaries.insert(id, Arc::default());
        self.log.push(OpRecord::TemporaryCreated { temporary: id });
        Ok(Temporary::new(id))
    }

    /// The temporary keyspace `temporary`, or why there is none: a store that is down serves none, and a recovery
    /// dropped every one.
    pub fn temporary(&self, temporary: &Temporary) -> Result<Arc<State>, StoreError> {
        if matches!(self.status, Status::Down { .. }) {
            return Err(StoreError::Poisoned());
        }
        self.temporaries
            .get(&temporary.id())
            .cloned()
            .ok_or_else(|| StoreError::UnknownTemporary(temporary.id()))
    }

    /// Writes `batch` to `temporary` under the batch faults: a failed write poisons the store, a crash takes the
    /// process down; a rebuild drawn to stop once it has written to its temporary keyspace stops after it.
    pub fn apply_temporary(
        &mut self,
        temporary: &Temporary,
        batch: WriteBatch,
    ) -> Result<(), StoreError> {
        self.refuse_unless_open("apply_temporary")?;
        let mut state = (*self.temporary(temporary)?).clone();
        self.maybe_slow();
        let id = temporary.id();
        if let Err(error) = batch.check_key_sizes() {
            self.log.push(OpRecord::TemporaryRefused {
                temporary: id,
                batch,
                error: error.clone(),
            });
            return Err(error);
        }
        match self.faults.batch() {
            BatchOutcome::Failed => {
                self.status = Status::Poisoned {
                    failed: Record::Batch {
                        entries: Vec::new(),
                        applied_ts: None,
                        removed_records: Vec::new(),
                        fenced: false,
                    },
                };
                self.log.push(OpRecord::TemporaryFailed {
                    temporary: id,
                    batch,
                    crashed: false,
                });
                Err(StoreError::WriteFailed(
                    "simulated failure during a temporary batch's persist".into(),
                ))
            }
            BatchOutcome::Crashed => {
                self.status = Status::Down {
                    power_loss: false,
                    in_flight: None,
                };
                self.log.push(OpRecord::TemporaryFailed {
                    temporary: id,
                    batch,
                    crashed: true,
                });
                Err(StoreError::WriteFailed(
                    "simulated process crash while a temporary batch was written".into(),
                ))
            }
            BatchOutcome::Written => {
                for entry in batch.entries() {
                    state.data.insert(
                        (entry.key().to_vec(), Reverse(entry.version())),
                        encode(entry, false),
                    );
                }
                self.temporaries.insert(id, Arc::new(state));
                self.log.push(OpRecord::TemporaryApplied {
                    temporary: id,
                    batch,
                });
                match self.rebuild_stops_at(RebuildStage::TemporaryWritten) {
                    Some((record, power_loss)) => {
                        Err(self.stop_rebuild(record, RebuildStage::TemporaryWritten, power_loss))
                    }
                    None => Ok(()),
                }
            }
        }
    }

    /// Drops `temporary` and what it holds.
    pub fn drop_temporary(&mut self, temporary: Temporary) -> Result<(), StoreError> {
        self.refuse_unless_open("drop_temporary")?;
        self.maybe_slow();
        let id = temporary.id();
        self.temporaries
            .remove(&id)
            .ok_or(StoreError::UnknownTemporary(id))?;
        self.log.push(OpRecord::TemporaryDropped { temporary: id });
        Ok(())
    }
}
