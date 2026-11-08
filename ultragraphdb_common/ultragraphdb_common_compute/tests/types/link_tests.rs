/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's one connection to a partition: closed by a frame compute refuses or by a partition hung under a read given up, and
//! dialled again by the next read; kept by a partition that goes on answering (`PROTOCOL.md` §11.12, §13.14).

use std::time::Duration;

use ultragraphdb_common_compute::LookupAt;
use ultragraphdb_common_role::{OpenRequests, RoleClient};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_store_quads::term_key;
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_cluster::{ApartCluster, links_holding_streams, run, times};
use crate::apart::apart_partition::Script;
use crate::apart::apart_reads::{
    P, TENANT, TWO_BY_TWO, answer_on, cancel, hold_lists, id, list_of, name, of_p, rows_listed,
    send_read,
};

/// The predicate a lookup names, and the subject its value index lists.
const NAME: &str = "http://ex.org/name";
const SUBJECT: &str = "http://ex.org/s";

/// The token a lookup names, the `term` tokenizer's.
const TOKEN: (u8, &[u8]) = (1, b"ann");

/// The message a link's close logs.
const CLOSES: &str = "a partition's connection closes";

/// The stall a link's waits derive from: a request is given up past 700 ms of silence and the partition hung past 1,400 ms.
const STALL: Duration = Duration::from_millis(700);

/// When a test gives up a silent read: past the 700 ms after which its link watches the partition, before the 1,400 ms after
/// which the read's own wait would close the link.
const GIVE_UP_AT: Duration = Duration::from_millis(1_000);

/// How long past giving up the test looks at the link: past the 1,400 ms after the request at which the link decides.
const DECIDED_AFTER: Duration = Duration::from_millis(800);

/// A cluster of one partition holding [`TWO_BY_TWO`], whose first Scan of `<p>`'s lists takes `script` once, and a connection to
/// compute reading by `stall`: the lists' key prefix.
async fn scripted(
    script: Script,
    stall: Duration,
) -> TestResult<(ApartCluster, RoleClient, Vec<u8>)> {
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
    partition.script(lists.clone(), script, 1);
    let client = cluster
        .compute(times(stall), OpenRequests::default())
        .await?;
    Ok((cluster, client, lists))
}

/// A frame breaking its Scan's order closes the connection, the read refused with code 17, and the next read dials anew.
#[test]
fn test_a_frame_out_of_order_closes_the_connection_and_the_next_read_dials() -> TestResult {
    run(async {
        let (cluster, mut client, _lists) =
            scripted(Script::BreakOrder, Duration::from_secs(1)).await?;
        let broken = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        let (code, _, _) = broken.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::Unavailable);
        let next = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(next.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert_eq!(cluster.partition(1)?.connections(), 2);
        Ok(())
    })
}

/// A read given up while its partition stays silent leaves the link to watch: a partition that sends nothing by the read's
/// wait is hung, its connection closes, and the next read dials anew.
#[test]
fn test_a_partition_silent_under_a_read_given_up_is_hung() -> TestResult {
    run(async {
        let (cluster, mut client, lists) = scripted(Script::Silent, STALL).await?;
        let partition = cluster.partition(1)?;
        let silent = send_read(&mut client, &of_p(0, Vec::new())).await?;
        partition.until_asked(&lists).await?;
        compio::time::sleep(GIVE_UP_AT).await;
        cancel(&mut client, silent).await?;
        compio::time::sleep(DECIDED_AFTER).await;
        let next = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(next.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert_eq!(partition.connections(), 2);
        Ok(())
    })
}

/// A read given up while its partition answers other reads leaves the link open: the partition is not hung.
#[test]
fn test_a_partition_answering_after_a_read_given_up_keeps_its_connection() -> TestResult {
    run(async {
        let (cluster, mut client, lists) = scripted(Script::Silent, STALL).await?;
        let partition = cluster.partition(1)?;
        let silent = send_read(&mut client, &of_p(0, Vec::new())).await?;
        partition.until_asked(&lists).await?;
        let answered = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(answered.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        compio::time::sleep(GIVE_UP_AT).await;
        cancel(&mut client, silent).await?;
        compio::time::sleep(DECIDED_AFTER).await;
        let next = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(next.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert_eq!(partition.connections(), 1);
        Ok(())
    })
}

/// Every stream a lookup opened leaves its link as the lookup ends: the core keeps its link to the partition for the reads after,
/// holding no stream of a lookup returned (`PROTOCOL.md` §11.3, §11.12).
#[test]
fn test_a_link_kept_holds_no_stream_of_a_lookup_returned() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let (predicate, subject) = (id(900)?, id(901)?);
        name(partition, NAME, predicate);
        name(partition, SUBJECT, subject);
        let key = QuadKey::ValueIndex {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            tokenizer: TOKEN.0,
            token: TOKEN.1.to_vec(),
        }
        .to_bytes();
        partition.put(key, list_of(&[subject]));
        let core = cluster.core(times(Duration::from_secs(1)));
        let found = core
            .value_index(TENANT, LookupAt::new(1, 1_000), NAME, TOKEN)
            .await
            .map_err(|reply| TestError::new(reply.message()))?;
        assert_eq!(found, [SUBJECT]);
        assert_eq!(
            links_holding_streams(&core).await,
            (1, 0),
            "one link, holding no stream"
        );
        assert_eq!(partition.connections(), 1);
        Ok(())
    })
}

/// A link's first close logs why it closed, naming its partition: a frame breaking its Scan's order closes the link, and the
/// warning carries the rule the read's refusal names (`PROTOCOL.md` §11.12, §13.14).
#[test]
fn test_a_links_first_close_logs_why_it_closed() -> TestResult {
    let capture = LogCapture::new();
    let refused = capture.capture(|| {
        run(async {
            let cluster = ApartCluster::start(1).await?;
            cluster
                .partition(1)?
                .script(term_key(TENANT, NAME), Script::BreakOrder, 1);
            let core = cluster.core(times(Duration::from_secs(1)));
            let answer = core
                .value_index(TENANT, LookupAt::new(1, 1_000), NAME, TOKEN)
                .await;
            answer.err().context("the lookup's refusal")
        })
    })?;
    assert_eq!(refused.code(), ErrorCode::Unavailable);
    let closes: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.message() == Some(CLOSES))
        .collect();
    assert_eq!(closes.len(), 1, "one close, logged once: {closes:?}");
    let close = closes.first().context("the close's warning")?;
    assert_eq!(close.level(), tracing::Level::WARN);
    assert_eq!(close.field("partition"), Some("1"));
    let why = close.field("why").context("why it closed")?;
    assert!(
        !why.is_empty() && refused.message().ends_with(why),
        "{why:?} is the rule {:?} names",
        refused.message()
    );
    Ok(())
}
