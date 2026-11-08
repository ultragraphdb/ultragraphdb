/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Scan answers its ranges in order, each from its own first key, one after the other (`PROTOCOL.md` §13.14), against a core of
//! `core_harness`.

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{Scan, ScanAt, ScanRange};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::TestResult;

use crate::types::partition_core::core_harness::{
    PARTITION, PRINCIPAL, Setup, TENANT, ex, on_core_in_time, scanned,
};

/// A list range, a run of two one-key dictionary ranges and a range of statements are answered in that order: the list's range,
/// the run in one frame through its last range, then the statements' range, the last frame alone flagged last.
#[test]
fn test_a_scan_answers_its_ranges_in_order_each_once() -> TestResult {
    on_core_in_time(
        "open-scan",
        Setup::default(),
        Address::parse("tcp://127.0.0.1:1")?,
        async |core| {
            let mut client = core.dial().await?;
            let forward = key_prefix(Keyspace::Forward, false, TENANT, None, None);
            let statements = key_prefix(Keyspace::Statements, false, TENANT, None, None);
            let (a, b) = (term_key(TENANT, &ex("a")), term_key(TENANT, &ex("b")));
            let (past_a, past_b) = ([a.as_slice(), &[0]].concat(), [b.as_slice(), &[0]].concat());
            let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, [0; 16], 0, 0);
            scan.push_range(ScanRange::new(PARTITION, &forward, &[]));
            scan.push_range(ScanRange::new(PARTITION, &a, &past_a));
            scan.push_range(ScanRange::new(PARTITION, &b, &past_b));
            scan.push_range(ScanRange::new(PARTITION, &statements, &[]));
            let (batches, refused) = scanned(&mut client, &scan).await?;
            assert!(refused.is_none(), "{refused:?}");
            let answered: Vec<(u32, u32, u8, bool)> = batches
                .iter()
                .map(|batch| (batch.range, batch.through, batch.end, batch.last))
                .collect();
            assert_eq!(
                answered,
                [(0, 0, 1, false), (1, 2, 1, false), (3, 3, 1, true)]
            );
            Ok(())
        },
    )
}
