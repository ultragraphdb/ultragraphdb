/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{Fanout, PartitionError, PartitionErrorEnum, ReverseQuery};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::cluster::{Cluster, TENANT, epoch, insert, pid};

/// `<a>` in partition 1 and `<c>` in partition 3 hold `knows` to `<b>`, homed in partition 2, the epoch closed: the
/// cluster and the names.
fn hub(partitions: u32) -> TestResult<(Cluster, [String; 4])> {
    let mut cluster = Cluster::even(partitions)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let c = cluster.name_in(3, "c")?;
    let knows = cluster.name_in(1, "knows")?;
    let _first: u64 = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    let _second: u64 = cluster.write(3, vec![insert(&c, &knows, &b)])?.version()?;
    let _closed: ultragraphdb_core_ids::Epoch = cluster.close()?;
    Ok((cluster, [a, b, c, knows]))
}

/// The reverse read of `<b>` through `knows` at the cluster's closed epoch.
fn query(cluster: &Cluster, names: &[String; 4]) -> TestResult<ReverseQuery> {
    let [_, b, _, knows] = names;
    let home = cluster.id_at(2, b)?.context("b's ID")?;
    let predicate = cluster.id_at(1, knows)?.context("knows's ID")?;
    Ok(ReverseQuery::new(
        pid(2)?,
        TENANT,
        Graph::Default,
        predicate,
        home,
        cluster.closed(),
    ))
}

/// A reverse read asks exactly the partitions `R(b)` names, here 1 and 3 of four, and returns each one's subject.
#[test]
fn test_a_reverse_read_asks_only_the_partitions_the_referrer_set_names() -> TestResult {
    let (cluster, names) = hub(4)?;
    let mut subjects = Vec::new();
    let asked = Fanout::new(cluster.parts()).subjects(&query(&cluster, &names)?, &mut subjects)?;
    assert_eq!(asked, vec![pid(1)?, pid(3)?]);
    let [a, _, c, _] = &names;
    let mut expected = vec![
        cluster.id_at(1, a)?.context("a's ID")?,
        cluster.id_at(3, c)?.context("c's ID")?,
    ];
    expected.sort();
    assert_eq!(subjects, expected);
    Ok(())
}

/// The reverse count sums the named partitions' own counts.
#[test]
fn test_a_reverse_count_sums_the_named_partitions() -> TestResult {
    let (cluster, names) = hub(3)?;
    assert_eq!(
        Fanout::new(cluster.parts()).count(&query(&cluster, &names)?)?,
        2
    );
    Ok(())
}

/// A partition the read must ask that has not closed its epoch fails it, naming that partition, with no rows.
#[test]
fn test_a_lagging_partition_fails_the_read_naming_it() -> TestResult {
    let (cluster, names) = hub(3)?;
    let query = query(&cluster, &names)?;
    let behind = cluster.closed().get() - 1;
    let closed = vec![
        cluster.closed(),
        cluster.closed(),
        ultragraphdb_core_ids::Epoch::new(behind)?,
    ];
    let mut subjects = vec![];
    let result = Fanout::new(cluster.parts())
        .with_closed(closed)
        .subjects(&query, &mut subjects);
    assert_eq!(result, Err(PartitionError::timed_out(3)));
    assert!(subjects.is_empty(), "no partial answer");
    Ok(())
}

/// A lookup by count asks the partitions of the tenant's run alone: one holding only ranges beside the run may lag,
/// and a partition of the run that lags fails it, naming that partition.
#[test]
fn test_a_lookup_by_count_waits_on_the_run_and_nothing_beside_it() -> TestResult {
    use ultragraphdb_cluster_placement::{NodeId, PlacementMap, TenantRun};
    use ultragraphdb_core_ids::{Epoch, Tenant};
    let run = TenantRun::of(Tenant::new(TENANT)).range();
    let mut placement = PlacementMap::even(2, NodeId::new(1))?;
    let at = epoch(2)?;
    let mut layout = Vec::new();
    for (range, owner) in placement.ranges_at(epoch(1)?) {
        if range.end() <= run.start() || range.start() >= run.end() {
            let touching = range.end() == run.start() || range.start() == run.end();
            layout.push((range, if touching { pid(2)? } else { owner }));
        }
    }
    layout.push((run, pid(1)?));
    placement.change(at, &layout)?;
    let cluster = Cluster::bare(&placement)?;
    let predicate = SystemId::from_u128((1 << 112) | (3 << 48) | (1 << 24))?;
    let lookup = |closed: Vec<Epoch>| {
        let mut found = Vec::new();
        Fanout::new(cluster.parts())
            .with_closed(closed)
            .reverse_counted(TENANT, Graph::Default, predicate, 1, at, &mut found)
    };
    lookup(vec![at, Epoch::ZERO])?;
    assert_eq!(
        lookup(vec![Epoch::ZERO, at]),
        Err(PartitionError::timed_out(1))
    );
    Ok(())
}

/// A read whose referrer set names a partition the reads were not given is refused naming it and the first bucket of the
/// group that resolved to it, not answered without it.
#[test]
fn test_a_partition_the_set_names_must_be_among_the_partitions() -> TestResult {
    let (cluster, names) = hub(3)?;
    let query = query(&cluster, &names)?;
    let without_three: Vec<_> = cluster.parts().iter().take(2).collect();
    let mut subjects = Vec::new();
    let refused = Fanout::over(without_three).subjects(&query, &mut subjects);
    assert!(
        matches!(&refused, Err(e) if matches!(e.0, PartitionErrorEnum::WrongPartition { owner: 3, .. })),
        "{refused:?}"
    );
    assert!(subjects.is_empty());
    Ok(())
}

/// A reverse count lookup over no partition reads nothing and finds nothing.
#[test]
fn test_a_reverse_count_lookup_over_no_partition_finds_nothing() -> TestResult {
    let (cluster, names) = hub(3)?;
    let [_, _, _, knows] = &names;
    let predicate = cluster.id_at(1, knows)?.context("knows's ID")?;
    let mut found = Vec::new();
    let none: Vec<_> = cluster.parts().iter().take(0).collect();
    Fanout::over(none).reverse_counted(
        TENANT,
        Graph::Default,
        predicate,
        1,
        cluster.closed(),
        &mut found,
    )?;
    assert!(found.is_empty());
    Ok(())
}

/// Whether `result` failed with the simulated store's error, as a quad read raises it.
fn store_failed<T: std::fmt::Debug>(result: &Result<T, PartitionError>) -> bool {
    matches!(result, Err(e) if matches!(&e.0, PartitionErrorEnum::Quad(_)))
}

/// A store failing under the home's read of the referrer set fails a reverse read and a reverse count with its error,
/// with no rows.
#[test]
fn test_a_store_failing_under_the_referrer_set_fails_the_read() -> TestResult {
    use ultragraphdb_store_sim::Fault;
    let (cluster, names) = hub(3)?;
    let query = query(&cluster, &names)?;
    let home = cluster.part(2)?.applier().quads().store();
    home.inject(Fault::FailNextRead);
    let mut subjects = Vec::new();
    let read = Fanout::new(cluster.parts()).subjects(&query, &mut subjects);
    assert!(store_failed(&read), "{read:?}");
    assert!(subjects.is_empty());
    home.inject(Fault::FailNextRead);
    let counted = Fanout::new(cluster.parts()).count(&query);
    assert!(store_failed(&counted), "{counted:?}");
    Ok(())
}

/// A store failing under a named partition's own reverse entries, restarted so its lists are read from the store, fails
/// a reverse read and a reverse count with its error, with no rows.
#[test]
fn test_a_store_failing_under_a_named_partitions_entries_fails_the_read() -> TestResult {
    use ultragraphdb_store_sim::Fault;
    let (mut cluster, names) = hub(3)?;
    let query = query(&cluster, &names)?;
    cluster.restart(1)?;
    let named = cluster.part(1)?.applier().quads().store();
    named.inject(Fault::FailNextScanAfter(0));
    let mut subjects = Vec::new();
    let read = Fanout::new(cluster.parts()).subjects(&query, &mut subjects);
    assert!(store_failed(&read), "{read:?}");
    assert!(subjects.is_empty());
    named.inject(Fault::FailNextScanAfter(0));
    let counted = Fanout::new(cluster.parts()).count(&query);
    assert!(store_failed(&counted), "{counted:?}");
    Ok(())
}

/// `friend` declared with an exact count index on `cluster`, and its ID at partition 1.
fn counted_friend(cluster: &mut Cluster) -> TestResult<SystemId> {
    use crate::utils::cluster::ex;
    use ultragraphdb_core_mapping::PredicateDeclaration;
    cluster.declare(
        "friend",
        PredicateDeclaration::new(ex("friend")).with_count_index(true),
    )?;
    cluster.id_at(1, "friend")?.context("friend's ID")
}

/// A lookup by count finds only the objects whose summed count is the one asked: an object of another count is not
/// found.
#[test]
fn test_a_lookup_by_count_leaves_out_an_object_of_another_count() -> TestResult {
    let mut cluster = Cluster::even(1)?;
    let friend = counted_friend(&mut cluster)?;
    let (one, two) = (cluster.name_in(1, "o")?, cluster.name_in(1, "t")?);
    let changes = vec![
        insert(&cluster.name_in(1, "s")?, "friend", &one),
        insert(&cluster.name_in(1, "u")?, "friend", &two),
        insert(&cluster.name_in(1, "v")?, "friend", &two),
    ];
    let _version: u64 = cluster.write(1, changes)?.version()?;
    let _closed = cluster.close()?;
    let mut found = Vec::new();
    Fanout::new(cluster.parts()).reverse_counted(
        TENANT,
        Graph::Default,
        friend,
        2,
        cluster.closed(),
        &mut found,
    )?;
    let two_id = cluster.id_at(1, &two)?.context("t's ID")?;
    let found_ids: Vec<_> = found
        .iter()
        .filter_map(ultragraphdb_core_terms::EncodedTerm::as_id)
        .collect();
    assert_eq!(found_ids, vec![two_id]);
    Ok(())
}

/// A count index holding more counts than one scan reads is read in pages, each resuming past the last: the object of
/// the highest count, on the second page, is found.
#[test]
fn test_a_lookup_by_count_reads_the_index_page_after_page() -> TestResult {
    let mut cluster = Cluster::even(1)?;
    let friend = counted_friend(&mut cluster)?;
    // One more count than a scan reads: objects of counts 1 to 257, every subject of a count also holding the lower.
    let counts: u32 = 257;
    let subjects: Vec<String> = (1..=counts).map(|i| format!("s{i}")).collect();
    let objects: Vec<String> = (1..=counts).map(|i| format!("o{i}")).collect();
    let mut changes = Vec::new();
    for (count, object) in (1..=counts as usize).zip(&objects) {
        for subject in subjects.iter().take(count) {
            changes.push(insert(subject, "friend", object));
        }
    }
    // Applied as set-up, in parts within a transaction's write set.
    for part in changes.chunks(4_096) {
        let _applied = cluster
            .part(1)?
            .applier()
            .apply(&crate::utils::cluster::changeset(part.to_vec()))?;
    }
    let _closed = cluster.close()?;
    let mut found = Vec::new();
    Fanout::new(cluster.parts()).reverse_counted(
        TENANT,
        Graph::Default,
        friend,
        u64::from(counts),
        cluster.closed(),
        &mut found,
    )?;
    let last = cluster
        .id_at(1, objects.last().context("an object")?)?
        .context("the last object's ID")?;
    let found_ids: Vec<_> = found
        .iter()
        .filter_map(ultragraphdb_core_terms::EncodedTerm::as_id)
        .collect();
    assert_eq!(found_ids, vec![last]);
    Ok(())
}
