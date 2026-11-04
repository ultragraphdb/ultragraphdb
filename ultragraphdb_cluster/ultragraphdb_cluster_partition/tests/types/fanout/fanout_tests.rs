/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{Fanout, PartitionError, ReverseQuery};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::cluster::{Cluster, NS, insert, pid};

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
        NS,
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
