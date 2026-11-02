/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{Close, SequencedTransaction, SequencerRecord};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_records_epoch_is_its_closes_or_its_transactions() -> TestResult {
    let close = SequencerRecord::Close(Close::new(Epoch::new(12)?, 0, vec![], vec![])?);
    assert_eq!(close.epoch().get(), 12);
    let transaction = SequencerRecord::Transaction(SequencedTransaction::new(
        Epoch::new(13)?,
        Epoch::new(11)?,
        vec![PartitionId::new(1)?],
        vec![],
    )?);
    assert_eq!(transaction.epoch().get(), 13);
    Ok(())
}
