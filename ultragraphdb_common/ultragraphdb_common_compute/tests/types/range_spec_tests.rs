/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The ranges a read asks a partition for: a whole range alone in its Scan, at the rows the read still wants, and one-key ranges
//! together, as many as those rows can fill, at no limit (`PROTOCOL.md` §13.13).

use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::{ApartCluster, run, times};
use crate::apart::apart_partition::SeenScan;
use crate::apart::apart_reads::{
    Lists, TENANT, answer_on, forward, hold_two_predicates, holding, id, of_p, of_s,
};

/// The one Scan whose first key starts with `prefix`.
fn scan_from(scans: &[SeenScan], prefix: &[u8]) -> TestResult<SeenScan> {
    let mut found = scans.iter().filter(|scan| scan.first.starts_with(prefix));
    let scan = found.next().context("a Scan from the prefix")?.clone();
    assert!(found.next().is_none(), "one Scan: {scans:?}");
    Ok(scan)
}

/// A predicate's forward lists are one whole range, scanned alone at the read's limit.
#[test]
fn test_a_whole_range_is_scanned_alone_at_the_limit() -> TestResult {
    const LISTS: Lists<'static> = &[
        ("http://ex.org/s1", &["http://ex.org/o1"]),
        ("http://ex.org/s2", &["http://ex.org/o2"]),
        ("http://ex.org/s3", &["http://ex.org/o3"]),
    ];
    run(async {
        let (cluster, mut client) = holding(LISTS, false).await?;
        let _answer = answer_on(&mut client, &of_p(2, Vec::new())).await?;
        // `holding` draws the predicate's ID first, from 100.
        let lists = key_prefix(
            Keyspace::Forward,
            false,
            TENANT,
            Some(Graph::Default),
            Some(id(100)?),
        );
        let scan = scan_from(&cluster.partition(1)?.scans(), &lists)?;
        assert_eq!((scan.ranges, scan.limit), (1, 2));
        Ok(())
    })
}

/// A subject's forward lists, one key each, are asked together in one Scan at no limit.
#[test]
fn test_one_key_ranges_are_scanned_together_at_no_limit() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let _iris = hold_two_predicates(cluster.partition(1)?)?;
        let mut client = cluster
            .compute(times(std::time::Duration::from_secs(1)), Default::default())
            .await?;
        let _answer = answer_on(&mut client, &of_s(0, Vec::new())).await?;
        // `hold_two_predicates` names the subject 200 and its first predicate 201.
        let first = forward(id(201)?, id(200)?);
        let scan = scan_from(&cluster.partition(1)?.scans(), &first)?;
        assert_eq!((scan.ranges, scan.limit), (2, 0));
        Ok(())
    })
}

/// A read wanting one more row asks one one-key range in a Scan.
#[test]
fn test_one_row_wanted_asks_one_one_key_range() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let _iris = hold_two_predicates(cluster.partition(1)?)?;
        let mut client = cluster
            .compute(times(std::time::Duration::from_secs(1)), Default::default())
            .await?;
        let _answer = answer_on(&mut client, &of_s(1, Vec::new())).await?;
        let first = forward(id(201)?, id(200)?);
        let scan = scan_from(&cluster.partition(1)?.scans(), &first)?;
        assert_eq!((scan.ranges, scan.limit), (1, 0));
        Ok(())
    })
}
