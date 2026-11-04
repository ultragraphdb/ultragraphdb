/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{NamedOrBlankNode, Term};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_core_mapping::Change;
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::PartitionShape;

/// How many commits a test draws.
const COMMITS: usize = 400;

fn shape(seed: u64, partition: u32, cross: u8) -> TestResult<(PartitionShape, PlacementMap)> {
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let shape = PartitionShape::new(seed, PartitionId::new(partition)?, &placement, 2, cross)
        .context("hubs")?;
    Ok((shape, placement))
}

/// The partition homing `iri`.
fn owner(placement: &PlacementMap, iri: &str) -> TestResult<u32> {
    let bucket = iri_bucket(0, iri).context("a bucket")?;
    Ok(placement.partition_of(bucket, Epoch::ZERO).get())
}

/// Every commit holds `relations` new nodes of the partition's own buckets, each with an edge and a string; no node
/// repeats.
#[test]
fn test_commits_write_new_subjects_of_the_partitions_own_buckets() -> TestResult {
    let (mut generator, placement) = shape(1, 3, 10)?;
    let mut nodes = std::collections::BTreeSet::new();
    for _ in 0..COMMITS {
        let commit = generator.next_commit();
        assert_eq!(
            commit.changes().len(),
            4,
            "an edge and a string each, for 2 relations"
        );
        for change in commit.changes() {
            let Change::Insert {
                subject: NamedOrBlankNode::NamedNode(subject),
                object,
                ..
            } = change
            else {
                return Err(ultragraphdb_tool_bdd::TestError::new(
                    "an insert with an IRI subject",
                ));
            };
            assert_eq!(owner(&placement, subject.as_str())?, 3, "{subject}");
            if matches!(object, Term::NamedNode(_)) {
                assert!(nodes.insert(subject.clone()), "{subject} repeats");
            }
        }
    }
    assert_eq!(nodes.len(), COMMITS * 2);
    Ok(())
}

/// The same seed gives the same commits; another seed or another partition other ones.
#[test]
fn test_one_seed_generates_the_same_commits() -> TestResult {
    let commits = |seed, partition| -> TestResult<Vec<_>> {
        let (mut generator, _) = shape(seed, partition, 10)?;
        Ok((0..COMMITS)
            .map(|_| format!("{:?}", generator.next_commit().changes()))
            .collect())
    };
    assert_eq!(commits(7, 1)?, commits(7, 1)?);
    assert_ne!(commits(7, 1)?, commits(8, 1)?);
    assert_ne!(commits(7, 1)?, commits(7, 2)?);
    Ok(())
}

/// How many of `commits` commits' edges point at another partition's entity, of how many, for the share `cross`.
fn crossing(cross: u8) -> TestResult<(u32, u32)> {
    let (mut generator, placement) = shape(5, 2, cross)?;
    let (mut crossed, mut edges) = (0, 0);
    for _ in 0..COMMITS {
        for change in generator.next_commit().changes() {
            if let Change::Insert {
                object: Term::NamedNode(object),
                ..
            } = change
            {
                edges += 1;
                crossed += u32::from(owner(&placement, object.as_str())? != 2);
            }
        }
    }
    Ok((crossed, edges))
}

/// The share of edges to other partitions' entities is the one asked for: none, all, and about a tenth over 800 edges,
/// within three standard deviations of the binomial's 80 (about 8.5 each).
#[test]
fn test_the_cross_share_is_the_one_configured() -> TestResult {
    assert_eq!(crossing(0)?.0, 0);
    let (all, edges) = crossing(100)?;
    assert_eq!(all, edges);
    let (tenth, of_tenth) = crossing(10)?;
    assert_eq!(of_tenth, 800);
    assert!((55..=105).contains(&tenth), "{tenth} of {of_tenth}");
    Ok(())
}

/// The relation type of each of `partition`'s first commits under `seed`, each commit one relation.
fn relations(seed: u64, partition: u32) -> TestResult<Vec<u32>> {
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let mut generator = PartitionShape::new(seed, PartitionId::new(partition)?, &placement, 1, 0)
        .context("hubs")?;
    (0..10)
        .map(|_| {
            let commit = generator.next_commit();
            let Some(Change::Insert { predicate, .. }) = commit.changes().first() else {
                return Err(ultragraphdb_tool_bdd::TestError::new("an insert"));
            };
            predicate
                .as_str()
                .rsplit("rel_t")
                .next()
                .and_then(|tail| tail.strip_suffix("_org"))
                .and_then(|digit| digit.parse().ok())
                .context("a relation's number")
        })
        .collect()
}

/// The generator's draws are pinned: a Python xorshift of shifts 13, 7 and 17, from the state `(seed ^ partition *
/// 0x9E3779B97F4A7C15) | 1`, three draws per relation, the first modulo 10 naming its type.
#[test]
fn test_the_draws_are_the_reference_xorshifts() -> TestResult {
    assert_eq!(relations(1, 1)?, [9, 0, 7, 2, 7, 3, 1, 7, 3, 3]);
    assert_eq!(relations(7, 3)?, [1, 9, 8, 2, 6, 4, 2, 6, 7, 3]);
    Ok(())
}

/// A commit's hub is drawn among the partition's: edges spread over many hubs, not one.
#[test]
fn test_edges_spread_over_the_hubs() -> TestResult {
    let (mut generator, _) = shape(3, 1, 0)?;
    let mut hubs = std::collections::BTreeSet::new();
    for _ in 0..COMMITS {
        for change in generator.next_commit().changes() {
            if let Change::Insert {
                object: Term::NamedNode(object),
                ..
            } = change
            {
                let _new = hubs.insert(object.clone());
            }
        }
    }
    // 800 draws among 64 hubs miss any one of them about once in 400,000 runs.
    assert_eq!(hubs.len(), 64);
    Ok(())
}
