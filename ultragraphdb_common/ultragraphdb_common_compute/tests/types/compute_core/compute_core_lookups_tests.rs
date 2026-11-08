/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The query core's lookups: a value index merged over every partition of a tenant's run, and a reverse count summed over them
//! (quad-reads, "A value-index lookup asks every partition"; index-maintenance, "A reverse count sums the referrers'
//! partitions").

use std::time::Duration;

use ultragraphdb_common_compute::LookupAt;
use ultragraphdb_core_ids::{Epoch, SystemId, Tenant};
use ultragraphdb_core_keys::{CountDirection, Graph, QuadKey};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_cluster::{ApartCluster, even_map, run, times};
use crate::apart::apart_reads::{TENANT, id, list_of, name};

const NAME: &str = "http://ex.org/name";

/// The token the lookup names, the `term` tokenizer's.
const TOKEN: (u8, &[u8]) = (1, b"ann");

/// A count of partitions placed evenly whose tenant's run two of them hold, and those two.
fn spread_over_two() -> TestResult<(u32, [u32; 2])> {
    for partitions in 2..=64 {
        let holders: Vec<u32> = even_map(partitions)?
            .holders_of_run(Tenant::new(TENANT), Epoch::ZERO)
            .iter()
            .map(|(holder, _)| holder.get())
            .collect();
        if let [first, second] = holders.as_slice() {
            return Ok((partitions, [*first, *second]));
        }
    }
    Err(TestError::new(
        "no even placement spreads the run over two partitions",
    ))
}

/// A lookup's refusal as a test reads it.
fn refused(reply: &ultragraphdb_protocol_wire::ErrorReply) -> TestError {
    TestError::new(format!("{:?}: {}", reply.code(), reply.message()))
}

/// The subjects each partition's value index lists are merged in the order of their IDs and named by their IRIs, and two
/// lookups on one core share one connection to each partition.
#[test]
fn test_a_value_index_lookup_merges_every_partition_of_the_run() -> TestResult {
    run(async {
        let (partitions, holders) = spread_over_two()?;
        let cluster = ApartCluster::start(partitions).await?;
        let predicate = id(900)?;
        let subjects: Vec<(String, SystemId)> = (1..=3u64)
            .map(|at| Ok((format!("http://ex.org/s{at}"), id(900 + at)?)))
            .collect::<TestResult<_>>()?;
        let key = QuadKey::ValueIndex {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            tokenizer: TOKEN.0,
            token: TOKEN.1.to_vec(),
        }
        .to_bytes();
        // The first and third subjects at one partition, the second at the other.
        for (holder, listed) in holders.iter().zip([[0usize, 2].as_slice(), [1].as_slice()]) {
            let partition = cluster.partition(*holder)?;
            name(partition, NAME, predicate);
            let ids: Vec<SystemId> = listed
                .iter()
                .map(|at| {
                    let (iri, held) = subjects.get(*at).context("a subject")?;
                    name(partition, iri, *held);
                    Ok(*held)
                })
                .collect::<TestResult<_>>()?;
            partition.put(key.clone(), list_of(&ids));
        }
        let core = cluster.core(times(Duration::from_secs(1)));
        let at = LookupAt::new(1, 1_000);
        for _ in 0..2 {
            let found = core
                .value_index(TENANT, at, NAME, TOKEN)
                .await
                .map_err(|reply| refused(&reply))?;
            let expected: Vec<String> = subjects.iter().map(|(iri, _)| iri.clone()).collect();
            assert_eq!(found, expected);
        }
        for holder in holders {
            assert_eq!(
                cluster.partition(holder)?.connections(),
                1,
                "partition {holder}"
            );
        }
        Ok(())
    })
}

/// An object's reverse count is the sum of every partition's count of it: the objects whose sum is the count asked are named,
/// in the order of their IDs, and no other.
#[test]
fn test_a_reverse_count_lookup_sums_every_partition_of_the_run() -> TestResult {
    run(async {
        let (partitions, [first, second]) = spread_over_two()?;
        let cluster = ApartCluster::start(partitions).await?;
        let predicate = id(950)?;
        let [b, c, d] = [id(951)?, id(952)?, id(953)?];
        let counted = |count: u32| {
            QuadKey::CountIndex {
                tenant: TENANT,
                graph: Graph::Default,
                predicate,
                direction: CountDirection::Reverse,
                count,
            }
            .to_bytes()
        };
        // `<b>` 2 + 1, `<c>` 3 at one partition, `<d>` 1 at the other.
        let one = cluster.partition(first)?;
        name(one, NAME, predicate);
        name(one, "http://ex.org/b", b);
        name(one, "http://ex.org/c", c);
        one.put(counted(2), list_of(&[b]));
        one.put(counted(3), list_of(&[c]));
        let other = cluster.partition(second)?;
        name(other, NAME, predicate);
        name(other, "http://ex.org/b", b);
        name(other, "http://ex.org/d", d);
        other.put(counted(1), list_of(&[b, d]));
        let core = cluster.core(times(Duration::from_secs(1)));
        let found = core
            .reverse_counted(TENANT, LookupAt::new(1, 1_000), NAME, 3)
            .await
            .map_err(|reply| refused(&reply))?;
        assert_eq!(found, ["http://ex.org/b", "http://ex.org/c"]);
        Ok(())
    })
}

/// A partition whose dictionary does not name the predicate holds no list of it and adds nothing, and a partition answering a
/// list a key per frame is read to its last frame.
#[test]
fn test_a_lookup_skips_a_partition_not_naming_its_predicate() -> TestResult {
    run(async {
        let (partitions, [first, second]) = spread_over_two()?;
        let cluster = ApartCluster::start(partitions).await?;
        let predicate = id(960)?;
        let subject = id(961)?;
        let one = cluster.partition(first)?;
        one.frame_per_key();
        name(one, NAME, predicate);
        name(one, "http://ex.org/s", subject);
        let indexed = QuadKey::ValueIndex {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            tokenizer: TOKEN.0,
            token: TOKEN.1.to_vec(),
        };
        one.put(indexed.to_bytes(), list_of(&[subject]));
        let counted = QuadKey::CountIndex {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            direction: CountDirection::Reverse,
            count: 1,
        };
        one.put(counted.to_bytes(), list_of(&[subject]));
        let core = cluster.core(times(Duration::from_secs(1)));
        let at = LookupAt::new(1, 1_000);
        let found = core
            .value_index(TENANT, at, NAME, TOKEN)
            .await
            .map_err(|reply| refused(&reply))?;
        assert_eq!(found, ["http://ex.org/s"]);
        let summed = core
            .reverse_counted(TENANT, at, NAME, 1)
            .await
            .map_err(|reply| refused(&reply))?;
        assert_eq!(summed, ["http://ex.org/s"]);
        assert!(
            cluster.partition(second)?.connections() > 0,
            "the other partition was asked"
        );
        Ok(())
    })
}

/// What a partition holds that does not read refuses the lookup with code 17, naming it: a term's entry holding no ID, an ID's
/// entry holding no text, an index list naming an object by no ID.
#[test]
fn test_a_lookup_of_what_does_not_read_is_refused() -> TestResult {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{ErrorCode, ObjectRecordsBuilder};
    use ultragraphdb_store_quads::{id_key, term_key};

    run(async {
        let predicate = id(970)?;
        let subject = id(971)?;
        let indexed = QuadKey::ValueIndex {
            tenant: TENANT,
            graph: Graph::Default,
            predicate,
            tokenizer: TOKEN.0,
            token: TOKEN.1.to_vec(),
        }
        .to_bytes();
        let mut literal = ObjectRecordsBuilder::new();
        let object =
            EncodedTerm::literal(oxrdf::Literal::new_simple_literal("x").as_ref(), |_| {
                Err::<SystemId, _>(ultragraphdb_core_terms::TermError::malformed(
                    Vec::new(),
                    "no datatype by ID",
                ))
            })?
            .into_term();
        literal.push(object.as_bytes(), &[]);
        let broken: [(Vec<u8>, Vec<u8>, &str); 3] = [
            (term_key(TENANT, NAME), vec![1, 2, 3], "holds no ID"),
            (id_key(TENANT, subject), vec![0xFF, 0xFE], "holds no text"),
            (
                indexed.clone(),
                literal.value().to_vec(),
                "naming no system ID",
            ),
        ];
        for (key, value, why) in broken {
            let cluster = ApartCluster::start(1).await?;
            let one = cluster.partition(1)?;
            name(one, NAME, predicate);
            name(one, "http://ex.org/s", subject);
            one.put(indexed.clone(), list_of(&[subject]));
            one.put(key, value);
            let core = cluster.core(times(Duration::from_secs(1)));
            let refusal = core
                .value_index(TENANT, LookupAt::new(1, 1_000), NAME, TOKEN)
                .await
                .err()
                .context("the lookup's refusal")?;
            assert_eq!(refusal.code(), ErrorCode::Unavailable);
            assert!(refusal.message().contains(why), "{}", refusal.message());
        }
        Ok(())
    })
}
