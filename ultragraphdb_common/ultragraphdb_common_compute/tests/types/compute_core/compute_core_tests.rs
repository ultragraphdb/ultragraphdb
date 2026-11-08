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

/// Times whose answer bound is 1 s: a peer silent past it is hung, and a dial on a loaded host comes well within it.
fn short_times() -> ultragraphdb_common_compute::ReadTimes {
    ultragraphdb_common_compute::ReadTimes::new(
        std::time::Duration::from_millis(500),
        std::time::Duration::from_secs(60),
    )
}

/// How long a slow frontend takes to answer: well past [`short_times`]'s bound.
const SLOW: std::time::Duration = std::time::Duration::from_secs(3);

/// The refusal of a read of `list` on `played`: its code and message.
async fn refusal_of(played: &PlayedCluster, list: &OneList) -> TestResult<(ErrorCode, String)> {
    let mut client = played.client().await?;
    let answer = read(&mut client, &snapshot_of(&bound_list(list))?).await?;
    answer.refusal.context("the read's refusal")
}

/// A frontend that answers compute's Placement or Members past the answer bound is hung: the read is refused with code 17,
/// naming what did not come.
#[test]
fn test_a_frontend_silent_past_the_bound_refuses_the_read() -> TestResult {
    run(async {
        for placement in [true, false] {
            let played = PlayedCluster::start(1, short_times()).await?;
            let list = OneList::new("subject", "predicate", 2)?;
            list.hold_in(played.partition(1)?);
            if placement {
                played.frontend().delay_placement(SLOW);
            } else {
                played.frontend().delay_members(SLOW);
            }
            let (code, message) = refusal_of(&played, &list).await?;
            assert_eq!(code, ErrorCode::Unavailable);
            let what = if placement {
                "placement map"
            } else {
                "members"
            };
            assert!(
                message.contains(what) && message.contains("did not come"),
                "{message}"
            );
        }
        Ok(())
    })
}

/// A partition the frontend's Members does not list, lists at an address that does not parse, or lists where nothing listens,
/// refuses a read reaching it with code 17, naming why; where nothing listens, the next read asks the Members again.
#[test]
fn test_a_partition_not_reached_refuses_the_read() -> TestResult {
    use ultragraphdb_protocol_transport::Address;
    use ultragraphdb_protocol_transport_compio::Listener;

    use crate::played::played_data::iri_homed;

    run(async {
        // Nothing listens at an address once its listener is gone.
        let closed = {
            let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
            listener.local_address()?.to_string()
        };
        let listed = [
            (None, "has joined no storage process"),
            (Some("not an address".to_string()), "the members"),
            (Some(closed), "the connection to partition 2"),
        ];
        for (address, why) in listed {
            let played = PlayedCluster::start_placed(even_ranges(2)?, 1, quiet_times()).await?;
            if let Some(address) = &address {
                played.frontend().list(2, address);
            }
            let homed = iri_homed("subject-", 2, 2)?;
            let stem = homed
                .strip_prefix("http://example.org/")
                .context("a subject under http://example.org/")?;
            let list = OneList::new(stem, "predicate", 1)?;
            let (code, message) = refusal_of(&played, &list).await?;
            assert_eq!(code, ErrorCode::Unavailable);
            assert!(message.contains(why), "{message}");
            let asked = played.frontend().members_asked();
            let _again = refusal_of(&played, &list).await?;
            assert_eq!(
                played.frontend().members_asked(),
                asked + 1,
                "the next read asks where partition 2 listens again"
            );
        }
        Ok(())
    })
}

/// Reads arriving while their core dials a partition wait on that one dial: both are answered over the one connection, and the
/// frontend is asked where the partition listens once.
#[test]
fn test_reads_arriving_during_a_dial_share_it() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(partition);
        played
            .frontend()
            .delay_members(std::time::Duration::from_millis(200));
        let (mut first, mut second) = (played.client().await?, played.client().await?);
        let read_one = snapshot_of(&bound_list(&list))?;
        let (one, two) =
            futures::future::join(read(&mut first, &read_one), read(&mut second, &read_one)).await;
        for answer in [one?.answered()?, two?.answered()?] {
            assert_eq!(answer.column(0), list.object_iris());
        }
        assert_eq!(partition.connections(), 1);
        assert_eq!(played.frontend().members_asked(), 1);
        Ok(())
    })
}

/// A Members listing compute processes beside the partitions names where each partition listens alone: the read reaches its
/// partition.
#[test]
fn test_members_listing_compute_processes_reach_the_partition() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        played.frontend().list_compute("127.0.0.1:1");
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(played.partition(1)?);
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound_list(&list))?)
            .await?
            .answered()?;
        assert_eq!(answer.column(0), list.object_iris());
        Ok(())
    })
}
