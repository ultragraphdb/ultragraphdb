/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::{ApartCluster, even_map, run, times};
use crate::apart::apart_reads::{TENANT, answer_on, read_of};

/// A read-write transaction's read of a subject another partition homes is refused with code 23, whose `detail` holds the
/// transaction's partition in bits 0–31 and the other in bits 32–63 (`PROTOCOL.md` §6, code 23).
#[test]
fn test_a_cross_partition_read_names_both_partitions_in_its_detail() -> TestResult {
    run(async {
        let map = even_map(2)?;
        let subject = (0..10_000)
            .map(|at| format!("http://ex.org/s{at}"))
            .find(|iri| {
                iri_bucket(TENANT, iri)
                    .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == 2)
            })
            .context("a subject partition 2 homes")?;
        let cluster = ApartCluster::start(2).await?;
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let read = read_of(
            (false, Some(&subject), Some("http://ex.org/p")),
            0,
            Vec::new(),
        );
        let answer = answer_on(&mut client, &read).await?;
        let (code, detail, _) = answer.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::CrossPartition);
        // Partition 1, the transaction's, in the low half; partition 2 in the high half.
        assert_eq!(detail, 1 + 2 * (1u64 << 32));
        Ok(())
    })
}
