/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One core's reads over its links (`PROTOCOL.md` §11.12; design D2): the placement map it holds once fetched at the system
//! group, which this stage fixes as the even map.

use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{CLUSTER, PlayedCluster, quiet_times, run};
use crate::played::played_data::OneList;
use crate::played::played_read::{Bound, read, snapshot_of};
use crate::played::played_system::even_ranges;

/// The pattern binding `list`'s subject and predicate.
fn bound_list(list: &OneList) -> Bound {
    Bound {
        subject: Some(list.subject.iri.clone()),
        predicate: Some(list.predicate.iri.clone()),
        object: None,
    }
}

/// A system group answering a placement other than the even map this stage holds, its ranges the even map's but each held by
/// another partition, refuses every read with code 17.
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
        let mut client = played.client();
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
        let mut client = played.client();
        let answer = read(&mut client, &snapshot_of(&bound_list(&list))?)
            .await?
            .answered()?;
        let named: Vec<u64> = answer.batches.iter().map(|batch| batch.cluster).collect();
        assert_eq!(named, vec![CLUSTER; answer.batches.len()]);
        Ok(())
    })
}

/// Times whose answer bound is 1 s: a peer silent past it is hung, and a dial on a loaded host comes well within it.
fn short_times() -> ultragraphdb_common_compute::ReadTimes {
    ultragraphdb_common_compute::ReadTimes::new(
        std::time::Duration::from_millis(500),
        std::time::Duration::from_secs(60),
    )
}

/// How long a slow system group takes to answer: well past [`short_times`]'s bound.
const SLOW: std::time::Duration = std::time::Duration::from_secs(3);

/// The refusal of a read of `list` on `played`: its code and message.
async fn refusal_of(played: &PlayedCluster, list: &OneList) -> TestResult<(ErrorCode, String)> {
    let mut client = played.client();
    let answer = read(&mut client, &snapshot_of(&bound_list(list))?).await?;
    answer.refusal.context("the read's refusal")
}
/// A system group that answers compute's Placement past the answer bound is hung: the read is refused with code 17, naming the
/// placement map that did not come. Members is the core's links' to read, not the core's.
#[test]
fn test_a_system_group_silent_on_placement_past_the_bound_refuses_the_read() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, short_times()).await?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(played.partition(1)?);
        played.system().delay_placement(SLOW);
        let (code, message) = refusal_of(&played, &list).await?;
        assert_eq!(code, ErrorCode::Unavailable);
        assert!(
            message.contains("placement map") && message.contains("did not come"),
            "{message}"
        );
        Ok(())
    })
}
