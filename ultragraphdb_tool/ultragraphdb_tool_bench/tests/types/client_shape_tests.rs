/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{PatternSlot, RequestTerm, TermTable, WriteChange};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, CLIENT_TENANT, ClientShape};

/// The partition `placement` gives `iri`.
fn partition(placement: &PlacementMap, iri: &str) -> TestResult<u32> {
    let bucket = iri_bucket(CLIENT_TENANT, iri).context("a bucket")?;
    Ok(placement.partition_of(bucket, Epoch::ZERO).get())
}

/// The IRI at `index` of `table`.
fn iri_at(table: &TermTable, index: u32) -> TestResult<String> {
    match table.get(index).context("a term")? {
        RequestTerm::Iri(iri) => Ok(iri.clone()),
        other @ (RequestTerm::BlankNode(_)
        | RequestTerm::Literal { .. }
        | RequestTerm::LanguageLiteral { .. }
        | RequestTerm::Triple { .. }) => Err(TestError::new(format!("an IRI, not {other:?}"))),
    }
}

/// Every commit writes its relations as an edge to a hub and the string "x", each of a new subject; every subject and
/// hub of a commit lies in one partition, and the partitions drawn cover all of them.
#[test]
fn test_each_commit_writes_new_nodes_and_their_hubs_in_one_partition() -> TestResult {
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let mut shape = ClientShape::new("t", 3, 5, (4, 40), 7)?;
    let (mut seen_subjects, mut seen_partitions) = (BTreeSet::new(), BTreeSet::new());
    for _ in 0..40 {
        let (table, changes) = shape.next_changes()?;
        assert_eq!(changes.len(), 10, "two changes per relation");
        let mut partitions = BTreeSet::new();
        for pair in changes.chunks(2) {
            let [
                WriteChange::Insert {
                    subject,
                    predicate,
                    object,
                    graph: None,
                },
                WriteChange::Insert {
                    subject: same,
                    predicate: w,
                    object: x,
                    graph: None,
                },
            ] = pair
            else {
                return Err(TestError::new(format!("an edge and its string: {pair:?}")));
            };
            assert_eq!(subject, same);
            assert_eq!(iri_at(&table, *predicate)?, ClientShape::iri("linked"));
            assert_eq!(iri_at(&table, *w)?, ClientShape::iri("w"));
            assert!(
                matches!(table.get(*x), Some(RequestTerm::Literal { lexical, datatype: None }) if lexical == "x")
            );
            let (node, hub) = (iri_at(&table, *subject)?, iri_at(&table, *object)?);
            assert!(
                node.contains("/t/c3/n") && hub.contains("/t/h"),
                "{node} {hub}"
            );
            assert!(seen_subjects.insert(node.clone()), "{node} is new");
            partitions.extend([partition(&placement, &node)?, partition(&placement, &hub)?]);
        }
        assert_eq!(
            partitions.len(),
            1,
            "one partition per commit: {partitions:?}"
        );
        seen_partitions.extend(partitions);
        let subjects: Vec<String> = shape.subjects().to_vec();
        assert_eq!(subjects.len(), 5);
        assert!(subjects.iter().all(|s| seen_subjects.contains(s)));
    }
    assert_eq!(seen_partitions, BTreeSet::from([1, 2, 3, 4]));
    Ok(())
}

/// A DirectWrite and a Write carry the commit in the root tenant; two connections name different subjects and the same
/// hubs.
#[test]
fn test_connections_share_hubs_and_name_their_own_subjects() -> TestResult {
    let mut first = ClientShape::new("run", 0, 1, (1, 2), 1)?;
    let mut second = ClientShape::new("run", 1, 1, (1, 2), 1)?;
    let direct = first.next_direct()?;
    assert_eq!(direct.tenant(), CLIENT_TENANT);
    let _write = second.next_write()?;
    assert_ne!(first.subjects(), second.subjects());
    let hubs = |shape: &mut ClientShape| -> TestResult<BTreeSet<String>> {
        let mut hubs = BTreeSet::new();
        for _ in 0..50 {
            let (table, changes) = shape.next_changes()?;
            if let Some(WriteChange::Insert { object, .. }) = changes.first() {
                let _new = hubs.insert(iri_at(&table, *object)?);
            }
        }
        Ok(hubs)
    };
    assert_eq!(hubs(&mut first)?, hubs(&mut second)?);
    assert!(first.draw_below(3) < 3 && first.draw_below(0) == 0);
    Ok(())
}

#[test]
fn test_a_read_binds_its_subject_and_the_string() -> TestResult {
    let read = ClientShape::read_of("http://ex.org/s", 5);
    assert_eq!((read.transaction(), read.limit()), (5, 0));
    let pattern = read.pattern();
    assert_eq!(pattern.graph(), PatternSlot::DefaultGraph);
    let PatternSlot::Bound(subject) = pattern.subject() else {
        return Err(TestError::new("a bound subject"));
    };
    assert_eq!(iri_at(read.terms(), subject)?, "http://ex.org/s");
    let PatternSlot::Bound(predicate) = pattern.predicate() else {
        return Err(TestError::new("a bound predicate"));
    };
    assert_eq!(iri_at(read.terms(), predicate)?, ClientShape::iri("w"));
    assert_eq!(pattern.object(), PatternSlot::Unbound);
    Ok(())
}

#[test]
fn test_hubs_leaving_a_partition_without_one_are_refused() {
    let refused = ClientShape::new("t", 0, 1, (8, 1), 1);
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains("without a hub"))),
        "{refused:?}"
    );
    assert!(
        ClientShape::new("t", 0, 1, (0, 1), 1).is_err(),
        "no partitions"
    );
}

/// The literal at `index` of `table`.
fn literal_at(table: &TermTable, index: u32) -> TestResult<String> {
    match table.get(index).context("a term")? {
        RequestTerm::Literal {
            lexical,
            datatype: None,
        } => Ok(lexical.clone()),
        other @ (RequestTerm::Iri(_)
        | RequestTerm::BlankNode(_)
        | RequestTerm::Literal { .. }
        | RequestTerm::LanguageLiteral { .. }
        | RequestTerm::Triple { .. }) => {
            Err(TestError::new(format!("a plain literal, not {other:?}")))
        }
    }
}

/// The parts of an insert in the default graph.
fn inserted(change: &WriteChange) -> TestResult<(u32, u32, u32)> {
    match change {
        WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object,
        } => Ok((*subject, *predicate, *object)),
        other @ (WriteChange::Insert { .. }
        | WriteChange::InsertEdge { .. }
        | WriteChange::Delete { .. }
        | WriteChange::DeleteStatement { .. }
        | WriteChange::DeletePattern { .. }
        | WriteChange::SetProperty { .. }
        | WriteChange::DeleteProperty { .. }
        | WriteChange::Declare(_)) => Err(TestError::new(format!(
            "an insert in the default graph, not {other:?}"
        ))),
    }
}

/// A commit in a partition named lies in it, and its strings are as long as asked.
#[test]
fn test_a_commit_in_a_named_partition_lies_there_with_its_strings() -> TestResult {
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let mut shape = ClientShape::new("t", 0, 3, (4, 40), 2)?.with_value_bytes(5);
    for at in 0..4u32 {
        let (table, changes) = shape.changes_in(at as usize)?;
        assert_eq!(changes.len(), 6);
        for change in changes.iter() {
            let (subject, _, _) = inserted(change)?;
            assert_eq!(partition(&placement, &iri_at(&table, subject)?)?, at + 1);
        }
        let (_, _, value) = inserted(changes.get(1).context("the string")?)?;
        assert_eq!(literal_at(&table, value)?, "xxxxx");
    }
    Ok(())
}

/// A scan reads every `w`, a span this connection's rare predicate, which its span write gives one new subject in each
/// partition.
#[test]
fn test_a_scan_reads_every_string_and_a_span_one_subject_per_partition() -> TestResult {
    let scan = ClientShape::scan_of(3);
    assert_eq!((scan.transaction(), scan.limit()), (3, 0));
    let pattern = scan.pattern();
    assert_eq!(pattern.subject(), PatternSlot::Unbound);
    assert_eq!(pattern.object(), PatternSlot::Unbound);
    let PatternSlot::Bound(w) = pattern.predicate() else {
        return Err(TestError::new("a bound predicate"));
    };
    assert_eq!(iri_at(scan.terms(), w)?, ClientShape::iri("w"));
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let mut shape = ClientShape::new("t", 6, 1, (4, 40), 2)?;
    assert!(shape.rare().contains("/t/c6/"), "{}", shape.rare());
    assert_ne!(
        shape.rare(),
        ClientShape::new("u", 6, 1, (4, 40), 2)?.rare(),
        "each run's own"
    );
    let mut partitions = BTreeSet::new();
    for write in shape.span_writes()? {
        let [change] = write.changes() else {
            return Err(TestError::new("one change per partition"));
        };
        let (subject, predicate, _) = inserted(change)?;
        assert_eq!(iri_at(write.terms(), predicate)?, shape.rare());
        let _new = partitions.insert(partition(&placement, &iri_at(write.terms(), subject)?)?);
    }
    assert_eq!(partitions, BTreeSet::from([1, 2, 3, 4]));
    let span = shape.span_of(4);
    assert_eq!(span.transaction(), 4);
    assert_eq!(span.pattern().subject(), PatternSlot::Unbound);
    let PatternSlot::Bound(rare) = span.pattern().predicate() else {
        return Err(TestError::new("a bound predicate"));
    };
    assert_eq!(iri_at(span.terms(), rare)?, shape.rare());
    Ok(())
}

/// The hot subject's seed names each predicate once; a hot write adds a new value under one of them, or under a predicate
/// of its own when fresh; a hot read binds the subject alone, up to its limit.
#[test]
fn test_hot_writes_and_reads_name_the_hot_subject() -> TestResult {
    let hot = ClientShape::hot();
    let seed = ClientShape::hot_seed(3)?;
    let mut predicates = BTreeSet::new();
    for change in seed.changes() {
        let (subject, predicate, value) = inserted(change)?;
        assert_eq!(iri_at(seed.terms(), subject)?, hot);
        assert_eq!(literal_at(seed.terms(), value)?, "seed");
        let _new = predicates.insert(iri_at(seed.terms(), predicate)?);
    }
    let held: BTreeSet<String> = (0..3).map(|p| ClientShape::iri(&format!("p{p}"))).collect();
    assert_eq!(predicates, held);
    let mut shape = ClientShape::new("t", 2, 1, (1, 1), 5)?;
    let mut values = BTreeSet::new();
    for fresh in [false, false, true, true] {
        let write = shape.next_hot(3, fresh)?;
        let [change] = write.changes() else {
            return Err(TestError::new("one change"));
        };
        let (subject, predicate, value) = inserted(change)?;
        assert_eq!(iri_at(write.terms(), subject)?, hot);
        let predicate = iri_at(write.terms(), predicate)?;
        assert_eq!(held.contains(&predicate), !fresh, "{predicate}");
        assert!(
            values.insert(literal_at(write.terms(), value)?),
            "a new value"
        );
    }
    assert_eq!(shape.hot_partition()?, 0);
    let read = ClientShape::hot_read(9, 2);
    assert_eq!((read.transaction(), read.limit()), (9, 2));
    let PatternSlot::Bound(subject) = read.pattern().subject() else {
        return Err(TestError::new("a bound subject"));
    };
    assert_eq!(iri_at(read.terms(), subject)?, hot);
    assert_eq!(read.pattern().predicate(), PatternSlot::Unbound);
    assert_eq!(read.pattern().object(), PatternSlot::Unbound);
    Ok(())
}
