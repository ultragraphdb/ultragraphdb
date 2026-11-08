/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a partition's read goes on: frame after frame, Scan after Scan, graph after graph, past a Scan stalled behind the merge,
//! and whether a read its limit stops hands a resume key (`PROTOCOL.md` §11.8, §11.12, §13.13).

use std::time::Duration;

use ultragraphdb_common_role::{OpenRequests, RoleClient};
use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::{ApartCluster, run, times};
use crate::apart::apart_partition::Script;
use crate::apart::apart_reads::{
    Lists, P, TENANT, TWO_BY_TWO, answer_on, forward, hold_lists, hold_two_predicates, holding, id,
    list_of, name, of_p, of_s, read_of, rows_listed,
};

/// Every row of every list, in the store's key order, each list's key and its end in frames of their own.
#[test]
fn test_a_read_returns_every_row_in_key_order_frame_after_frame() -> TestResult {
    run(async {
        let (_cluster, mut client) = holding(TWO_BY_TWO, true).await?;
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(answer.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert!(answer.resume.is_empty(), "a read with no limit ends whole");
        Ok(())
    })
}

/// A limit met by the partition's last row, its Scan ended, leaves nothing to resume: the read hands no resume key.
#[test]
fn test_a_limit_meeting_the_last_row_hands_no_resume_key() -> TestResult {
    run(async {
        let (_cluster, mut client) = holding(TWO_BY_TWO, false).await?;
        let answer = answer_on(&mut client, &of_p(4, Vec::new())).await?;
        assert_eq!(answer.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert!(
            answer.resume.is_empty(),
            "nothing is left past the last row"
        );
        Ok(())
    })
}

/// A limit stopping inside a page hands a resume key, and the read resumed there returns the rest.
#[test]
fn test_a_limit_inside_a_page_hands_a_resume_key_the_read_goes_on_from() -> TestResult {
    const LISTS: Lists<'static> = &[(
        "http://ex.org/s1",
        &["http://ex.org/o1", "http://ex.org/o2", "http://ex.org/o3"],
    )];
    run(async {
        let (_cluster, mut client) = holding(LISTS, false).await?;
        let rows = rows_listed(LISTS);
        let first = answer_on(&mut client, &of_p(2, Vec::new())).await?;
        assert_eq!(first.whole()?, rows.get(..2).context("two rows")?);
        assert!(!first.resume.is_empty(), "a row is left in the page");
        let rest = answer_on(&mut client, &of_p(2, first.resume)).await?;
        assert_eq!(rest.whole()?, rows.get(2..).context("the third row")?);
        assert!(rest.resume.is_empty(), "nothing is left past the last row");
        Ok(())
    })
}

const THREE_SUBJECTS: Lists<'static> = &[
    ("http://ex.org/s1", &["http://ex.org/o1"]),
    ("http://ex.org/s2", &["http://ex.org/o2"]),
    ("http://ex.org/s3", &["http://ex.org/o3"]),
];

/// A Scan stopped at the read's limit with a key of its range left: the read's rows fill its limit, its page is drained, and
/// the range left over is what its resume key goes on from.
#[test]
fn test_a_scan_stopped_at_its_limit_leaves_its_range_to_resume() -> TestResult {
    run(async {
        let (_cluster, mut client) = holding(THREE_SUBJECTS, false).await?;
        let rows = rows_listed(THREE_SUBJECTS);
        let first = answer_on(&mut client, &of_p(2, Vec::new())).await?;
        assert_eq!(first.whole()?, rows.get(..2).context("two rows")?);
        assert!(!first.resume.is_empty(), "a key of the range is left");
        let rest = answer_on(&mut client, &of_p(2, first.resume)).await?;
        assert_eq!(rest.whole()?, rows.get(2..).context("the third row")?);
        Ok(())
    })
}

/// A limit met while the Scan is still open hands a resume key, reading no frame past the rows it returns; the read resumed
/// there finds nothing more.
#[test]
fn test_a_scan_still_open_at_the_limit_hands_a_resume_key() -> TestResult {
    const LISTS: Lists<'static> = &[
        ("http://ex.org/s1", &["http://ex.org/o1"]),
        ("http://ex.org/s2", &["http://ex.org/o2"]),
    ];
    run(async {
        let (_cluster, mut client) = holding(LISTS, true).await?;
        let first = answer_on(&mut client, &of_p(2, Vec::new())).await?;
        assert_eq!(first.whole()?, rows_listed(LISTS).as_slice());
        assert!(!first.resume.is_empty(), "the Scan had not ended");
        let rest = answer_on(&mut client, &of_p(2, first.resume)).await?;
        assert!(rest.whole()?.is_empty(), "{:?}", rest.rows);
        assert!(rest.resume.is_empty(), "nothing is left");
        Ok(())
    })
}

/// A subject's predicates are read from their list, then each predicate's forward list.
#[test]
fn test_a_subject_read_reads_its_predicates_list_then_each_list() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let [p1, o1, p2, o2] = hold_two_predicates(cluster.partition(1)?)?;
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let answer = answer_on(&mut client, &of_s(0, Vec::new())).await?;
        let row = |p: &str, o: &str| vec![Some(p.to_string()), Some(o.to_string()), None];
        assert_eq!(answer.whole()?, [row(p1, o1), row(p2, o2)].as_slice());
        Ok(())
    })
}

/// A subject read stopped after its first predicate's row resumes inside the predicates' list, at the next predicate: the
/// row returned is not returned again.
#[test]
fn test_a_subject_read_resumed_goes_on_at_the_next_predicate() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let [p1, o1, p2, o2] = hold_two_predicates(cluster.partition(1)?)?;
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let row = |p: &str, o: &str| vec![Some(p.to_string()), Some(o.to_string()), None];
        let first = answer_on(&mut client, &of_s(1, Vec::new())).await?;
        assert_eq!(first.whole()?, [row(p1, o1)].as_slice());
        assert!(
            !first.resume.is_empty(),
            "the second predicate's list is left"
        );
        let rest = answer_on(&mut client, &of_s(1, first.resume)).await?;
        assert_eq!(rest.whole()?, [row(p2, o2)].as_slice());
        Ok(())
    })
}

/// A read of every graph walks the keyspace graph by graph, the cell of a named graph its IRI and the default graph's absent.
#[test]
fn test_a_read_of_every_graph_walks_each_graph_in_turn() -> TestResult {
    const G: &str = "http://ex.org/g";
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let graph = id(90)?;
        name(partition, G, graph);
        let p = hold_lists(
            partition,
            (Graph::Default, P),
            &[("http://ex.org/s1", &["http://ex.org/o1"])],
            100,
        )?;
        let (s2, o2) = (id(110)?, id(111)?);
        name(partition, "http://ex.org/s2", s2);
        name(partition, "http://ex.org/o2", o2);
        let named = crate::apart::apart_reads::forward_in(Graph::Named(graph), p, s2);
        partition.put(named.clone(), list_of(&[o2]));
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let answer = answer_on(&mut client, &read_of((true, None, Some(P)), 0, Vec::new())).await?;
        let default_row = vec![
            None,
            Some("http://ex.org/s1".to_string()),
            Some("http://ex.org/o1".to_string()),
            None,
        ];
        let named_row = vec![
            Some(G.to_string()),
            Some("http://ex.org/s2".to_string()),
            Some("http://ex.org/o2".to_string()),
            None,
        ];
        // The store's key order of the two lists decides which graph is walked first.
        let default_first = forward(p, id(101)?) < named;
        let expected = if default_first {
            [default_row, named_row]
        } else {
            [named_row, default_row]
        };
        assert_eq!(answer.whole()?, expected.as_slice());
        // Every graph's key lies within the tenant's forward keyspace, which the walk starts from.
        assert!(named.starts_with(&key_prefix(Keyspace::Forward, false, TENANT, None, None)));
        Ok(())
    })
}

/// A Scan stalled behind the merge, once between two keys and once inside a list, goes on by a new Scan from where it stopped,
/// every row returned once.
#[test]
fn test_a_scan_stalled_behind_the_merge_goes_on_where_it_stopped() -> TestResult {
    const LISTS: Lists<'static> = &[
        (
            "http://ex.org/s1",
            &["http://ex.org/o1", "http://ex.org/o2"],
        ),
        ("http://ex.org/s2", &["http://ex.org/o3"]),
        ("http://ex.org/s3", &["http://ex.org/o4"]),
    ];
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let p = hold_lists(partition, (Graph::Default, P), LISTS, 100)?;
        let lists = key_prefix(
            Keyspace::Forward,
            false,
            TENANT,
            Some(Graph::Default),
            Some(p),
        );
        partition.frame_per_key();
        partition.script(lists.clone(), Script::StallInList, 1);
        partition.script(lists, Script::StallPastKey, 1);
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(answer.whole()?, rows_listed(LISTS).as_slice());
        Ok(())
    })
}

/// A Scan of one-key ranges stalled past its first range's key goes on from the next range, the first's key not read again.
#[test]
fn test_a_stall_past_a_one_key_range_goes_on_at_the_next_range() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let [p1, o1, p2, o2] = hold_two_predicates(partition)?;
        partition.script(forward(id(201)?, id(200)?), Script::StallPastKey, 1);
        let mut client = cluster
            .compute(times(Duration::from_secs(1)), OpenRequests::default())
            .await?;
        let answer = answer_on(&mut client, &of_s(0, Vec::new())).await?;
        let row = |p: &str, o: &str| vec![Some(p.to_string()), Some(o.to_string()), None];
        assert_eq!(answer.whole()?, [row(p1, o1), row(p2, o2)].as_slice());
        Ok(())
    })
}

/// The first key of `<p>`'s lists in a cluster holding [`TWO_BY_TWO`], and a connection to compute.
async fn scripted(script: Script, times_fired: u32) -> TestResult<(ApartCluster, RoleClient)> {
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
    partition.script(lists, script, times_fired);
    let client = cluster
        .compute(times(Duration::from_secs(1)), OpenRequests::default())
        .await?;
    Ok((cluster, client))
}

/// A partition stalling again where the read went on from, having given nothing past it, refuses the read with its code 24.
#[test]
fn test_a_stall_again_where_the_read_went_on_from_refuses_the_read() -> TestResult {
    run(async {
        let (_cluster, mut client) = scripted(Script::StallInList, 0).await?;
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        let (code, _, _) = answer.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::TimedOut);
        Ok(())
    })
}

/// A Scan stalled before it gave any key refuses the read for what it is, its code 24 relayed.
#[test]
fn test_a_scan_stalled_before_any_key_refuses_the_read() -> TestResult {
    run(async {
        let (_cluster, mut client) = scripted(Script::Refuse(ErrorCode::TimedOut), 0).await?;
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        let (code, detail, _) = answer.refused.context("a refusal")?;
        assert_eq!((code, detail), (ErrorCode::TimedOut, 1));
        Ok(())
    })
}

/// A partition's refusal other than a stall is relayed as the read's.
#[test]
fn test_a_partition_refusing_a_list_scan_refuses_the_read() -> TestResult {
    run(async {
        let (_cluster, mut client) = scripted(Script::Refuse(ErrorCode::Unavailable), 0).await?;
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        let (code, _, message) = answer.refused.context("a refusal")?;
        assert_eq!(code, ErrorCode::Unavailable);
        assert_eq!(message, "a played refusal");
        Ok(())
    })
}

/// A Scan its limit stopped at keys that gave no row, as a forward list holding another vector gives none, goes on from the key
/// past its last: the read finds its one row further on.
#[test]
fn test_a_scan_stopped_by_its_limit_short_of_a_row_goes_on() -> TestResult {
    use ultragraphdb_core_terms::{FLOAT32_VECTOR, format_vector};
    use ultragraphdb_protocol_wire::RequestTerm;

    use crate::played::played_cluster::{PlayedCluster, quiet_times};
    use crate::played::played_data::{Term, forward, literal, records};
    use crate::played::played_read::{Bound, read, snapshot_of};

    crate::played::played_cluster::run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let predicate = Term::resource("http://example.org/p", 1)?;
        predicate.name_in(partition);
        let (other, held) = (
            format_vector([3.0].into_iter()),
            format_vector([1.0, 2.5].into_iter()),
        );
        let mut subjects = Vec::new();
        // The first subject's list holds another vector, the second's the one read.
        for (sequence, lexical) in [(2, &other), (3, &held)] {
            let subject = Term::resource(&format!("http://example.org/s{sequence}"), sequence)?;
            subject.name_in(partition);
            let value = literal(lexical, Some(FLOAT32_VECTOR))?;
            partition.put(
                forward(&predicate, &subject),
                records(&[(value.as_bytes().to_vec(), Vec::new())]),
            );
            subjects.push(subject);
        }
        let bound = Bound {
            subject: None,
            predicate: Some(predicate.iri.clone()),
            object: Some(RequestTerm::Literal {
                lexical: held,
                datatype: Some(FLOAT32_VECTOR.to_string()),
            }),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?.with_limit(1))
            .await?
            .answered()?;
        let second = subjects.get(1).context("a second subject")?;
        assert_eq!(answer.column(0), vec![Some(second.iri.clone())]);
        let limited = partition
            .scans()
            .iter()
            .filter(|scan| scan.limit == 1)
            .count();
        assert!(limited >= 2, "a second Scan went on: {limited}");
        Ok(())
    })
}
