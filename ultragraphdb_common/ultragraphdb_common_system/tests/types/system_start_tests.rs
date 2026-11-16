/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_system::SystemStart;
use ultragraphdb_tool_bdd::TestResult;

/// A start takes `transaction_ms` 10,000 unless set otherwise, the default of `PROTOCOL.md` §11.9, and the value it is set to.
#[test]
fn test_a_start_holds_its_transaction_limit() -> TestResult {
    let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 5)?, 1);
    assert_eq!(start.transaction_ms(), 10_000);
    assert_eq!(start.with_transaction_ms(25).transaction_ms(), 25);
    Ok(())
}
