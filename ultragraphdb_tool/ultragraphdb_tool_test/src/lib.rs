/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The helpers the store crates' test targets share: a store directory under `TEST_TMPDIR`, the crash writer's
//! harness, the store workload run on fjall beside the simulated store, and the changeset applier's workload on the
//! simulator. Each slow suite is split into test targets of their own, which Bazel runs in parallel, and each target
//! reaches these through this crate.

mod types;
mod utils;

pub use crate::types::changeset_workload::{
    ChangesetWorkload, INDEXES_MATCH_DATA, MODEL_AGREES, NO_UNEXPECTED_ERROR,
    RECORDS_MATCH_POSTINGS, REPLAY_AGREES, REVERSE_HOLDS_STATEMENTS, STATEMENT_READS_MATCH,
    STATISTICS_MATCH_DATA,
};
pub use crate::types::partition_workload::PartitionWorkload;
pub use crate::types::run::Run;
pub use crate::types::test_dir::TestDir;
pub use crate::utils::crash_harness::{
    DROP_PREFIX, DROP_STAGES, DROPPED_KEYS, KEYS_PER_BATCH, RECORD_STAGES, TEMPORARY_STAGES,
    acknowledged_rounds, batch_counts, drop_outcome, fresh_dir, kill_after, kill_at, open,
    raw_drop_state, raw_temporary_keyspaces, replay_round, versionless_counts,
};
pub use crate::utils::fjall_workload::run_both;
pub use crate::utils::test_store::{
    held, put, scanned, stored, tenant_key, versions, write, write_raw, write_raw_at, write_raw_in,
};
