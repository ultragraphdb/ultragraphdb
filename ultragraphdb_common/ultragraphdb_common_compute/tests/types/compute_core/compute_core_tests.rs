/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One core's connections to the frontend and the partitions (`PROTOCOL.md` §11.12): the placement map it holds once fetched,
//! which this stage fixes as the even map, where each partition listens, and its one connection to each.

use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{CLUSTER, PlayedCluster, quiet_times, run};
use crate::played::played_data::OneList;
use crate::played::played_frontend::even_ranges;
use crate::played::played_read::{Bound, read, snapshot_of};

/// The pattern binding `list`'s subject and predicate.
fn bound_list(list: &OneList) -> Bound {
    Bound {
        subject: Some(list.subject.iri.clone()),
        predicate: Some(list.predicate.iri.clone()),
        object: None,
    }
}

/// A frontend answering a placement other than the even map this stage holds, its ranges the even map's but each held by another
/// partition, refuses every read with code 17.
#[test]
fn test_a_placement_other_than_the_even_map_refuses_the_read() -> TestResult {
    run(async {
        let even = even_ranges(2)?;
        let swapped: Vec<(u32, u32, u32)> = even
            .iter()
            .map(|(start, end, partition)| (*start, *end, 3 - partition))
            .collect();
        assert_ne!(swapped, even);
        let played = PlayedCluster::start_placed(swapped, 2, quiet_times()).await?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(played.partition(1)?);
        list.hold_in(played.partition(2)?);
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound_list(&list))?).await?;
        let (code, message) = answer.refusal.context("the read's refusal")?;
        assert_eq!(code, ErrorCode::Unavailable);
        assert!(message.contains("placement"), "{message}");
        Ok(())
    })
}

/// Every batch of a read names the cluster in its header.
#[test]
fn test_every_batch_names_the_cluster() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(played.partition(1)?);
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound_list(&list))?)
            .await?
            .answered()?;
        let named: Vec<u64> = answer.batches.iter().map(|batch| batch.cluster).collect();
        assert_eq!(named, vec![CLUSTER; answer.batches.len()]);
        Ok(())
    })
}

/// The reads of one core share its one connection to a partition, and the frontend is asked where it listens once.
#[test]
fn test_the_reads_of_a_core_share_its_connection_to_a_partition() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(partition);
        let mut client = played.client().await?;
        for _ in 0..3 {
            let answer = read(&mut client, &snapshot_of(&bound_list(&list))?)
                .await?
                .answered()?;
            assert_eq!(answer.column(0), list.object_iris());
        }
        assert_eq!(partition.connections(), 1);
        assert_eq!(played.frontend().members_asked(), 1);
        Ok(())
    })
}
