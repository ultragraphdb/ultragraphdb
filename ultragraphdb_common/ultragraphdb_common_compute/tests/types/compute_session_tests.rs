/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend's connection at compute: each read's batches pushed on its stream as the stream has room, and a read the frontend
//! cancels dropped (`PROTOCOL.md` §11.3, §13.12).

use std::time::Duration;

use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::apart::apart_cluster::{ApartCluster, run, times};
use crate::apart::apart_partition::Script;
use crate::apart::apart_reads::{
    P, TENANT, TWO_BY_TWO, answer_on, cancel, hold_lists, of_p, rows_of, send_read,
};

/// A read whose rows take more than one of the client's frames is answered in several batches, each sent once the one before
/// has left and its stream holds credit.
#[test]
fn test_a_read_larger_than_a_frame_is_answered_batch_after_batch() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        // Four objects whose IRIs each take a third of the client's frame: no batch holds more than two of them.
        let long = |at: usize| format!("http://ex.org/{at}/{}", "o".repeat(MAX_FRAME_FLOOR / 3));
        let objects: Vec<String> = (1..=4).map(long).collect();
        let named: Vec<&str> = objects.iter().map(String::as_str).collect();
        let lists = [("http://ex.org/s1", named.as_slice())];
        let _p = hold_lists(cluster.partition(1)?, (Graph::Default, P), &lists, 100)?;
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let read = of_p(0, Vec::new()).with_client_limits(MAX_FRAME_FLOOR_U32, 4);
        let answer = answer_on(&mut client, &read).await?;
        let pairs: Vec<(&str, &str)> = named
            .iter()
            .map(|object| ("http://ex.org/s1", *object))
            .collect();
        assert_eq!(answer.whole()?, rows_of(&pairs).as_slice());
        assert!(answer.batches >= 2, "{} batches", answer.batches);
        Ok(())
    })
}

/// A read the frontend cancels while its partition is silent is dropped at once, so a stop no longer waits on it.
#[test]
fn test_a_cancelled_read_is_dropped_at_once() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let p = hold_lists(partition, (Graph::Default, P), TWO_BY_TWO, 100)?;
        let lists = key_prefix(
            Keyspace::Forward,
            false,
            TENANT,
            Some(Graph::Default),
            Some(p),
        );
        partition.script(lists.clone(), Script::Silent, 1);
        let open = OpenRequests::default();
        // A stall of 2 s: the silent Scan ends the read of itself 4 s on, long past the test's wait for the drop.
        let mut client = cluster
            .compute(times(Duration::from_secs(2)), open.clone())
            .await?;
        let stream = send_read(&mut client, &of_p(0, Vec::new())).await?;
        partition.until_asked(&lists).await?;
        assert_eq!(open.count(), 1, "the read is open");
        cancel(&mut client, stream).await?;
        let dropped = async {
            while open.count() != 0 {
                compio::time::sleep(Duration::from_millis(2)).await;
            }
        };
        compio::time::timeout(Duration::from_secs(1), dropped)
            .await
            .map_err(|_| TestError::new("the cancelled read was still open"))
    })
}
