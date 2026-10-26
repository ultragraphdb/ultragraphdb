/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{NamedOrBlankNode, Term};
use ultragraphdb_core_mapping::{Change, Declaration, PredicateKind};
use ultragraphdb_tool_bench::CommitShape;

/// The predicate, subject and object of every insert of `changes`.
fn inserts(changes: &[Change]) -> Vec<(String, NamedOrBlankNode, Term)> {
    changes
        .iter()
        .map(|c| match c {
            Change::Insert {
                subject,
                predicate,
                object,
                ..
            } => (
                predicate.as_str().to_string(),
                subject.clone(),
                object.clone(),
            ),
            other => panic!("{other:?} is no insert"),
        })
        .collect()
}

#[test]
fn test_the_declarations_are_the_dgraph_runs_ten_edge_types_and_the_hub_name() {
    let changeset = CommitShape::declarations(10);
    let declared: Vec<_> = changeset
        .changes()
        .iter()
        .map(|c| match c {
            Change::Declare(Declaration::Predicate(p)) => p.clone(),
            other => panic!("{other:?} is no predicate declaration"),
        })
        .collect();
    assert_eq!(declared.len(), 21);
    assert_eq!(declared[0].predicate(), &CommitShape::iri("rel_t0_org"));
    assert_eq!(declared[0].kind(), PredicateKind::Edge);
    assert_eq!(declared[1].predicate(), &CommitShape::iri("rel_t0_w"));
    assert_eq!(declared[1].kind(), PredicateKind::Property);
    assert_eq!(declared[1].datatype(), Some(&xsd::STRING.into_owned()));
    assert_eq!(declared[19].predicate(), &CommitShape::iri("rel_t9_w"));
    assert_eq!(declared[20].predicate(), &CommitShape::iri("name"));
    assert_eq!(declared[20].tokenizers(), ["exact"]);
}

#[test]
fn test_the_hubs_carry_their_names() {
    let shape = CommitShape::new(1, 1_000, 10, 7);
    let hubs = inserts(shape.hubs().changes());
    assert_eq!(hubs.len(), 1_000);
    assert_eq!(hubs[0].0, CommitShape::iri("name").as_str());
    assert_eq!(hubs[0].1, CommitShape::hub(0).into());
    assert_eq!(hubs[0].2.to_string(), "\"h0\"");
    assert_eq!(hubs[999].1, CommitShape::hub(999).into());
}

#[test]
fn test_a_commit_writes_new_records_of_one_type_each_linked_to_a_hub_with_a_string() {
    let mut shape = CommitShape::new(50, 1_000, 10, 7);
    let commit = inserts(shape.next_commit().changes());
    assert_eq!(commit.len(), 100);
    let org = &commit[0].0;
    let ty = org
        .trim_start_matches(CommitShape::iri("rel_t").as_str())
        .trim_end_matches("_org")
        .to_string();
    for (i, pair) in commit.chunks(2).enumerate() {
        let (link, value) = (&pair[0], &pair[1]);
        assert_eq!(link.1, CommitShape::iri(&format!("r{}", i + 1)).into());
        assert_eq!(link.0, CommitShape::iri(&format!("rel_t{ty}_org")).as_str());
        let Term::NamedNode(hub) = &link.2 else {
            panic!("{} is no hub", link.2)
        };
        let n: u64 = hub
            .as_str()
            .trim_start_matches(CommitShape::iri("h").as_str())
            .parse()
            .expect("a hub's number");
        assert!(n < 1_000);
        assert_eq!(value.1, link.1);
        assert_eq!(value.0, CommitShape::iri(&format!("rel_t{ty}_w")).as_str());
        assert_eq!(value.2.to_string(), "\"x\"");
    }
    // The next commit's records are new nodes.
    let next = inserts(shape.next_commit().changes());
    assert_eq!(next[0].1, CommitShape::iri("r51").into());
}

#[test]
fn test_a_seed_replays_its_commits_and_the_types_vary() {
    let mut a = CommitShape::new(1, 1_000, 10, 7);
    let mut b = CommitShape::new(1, 1_000, 10, 7);
    let predicate = |s: &mut CommitShape| inserts(s.next_commit().changes())[0].0.clone();
    let types: Vec<String> = (0..50).map(|_| predicate(&mut a)).collect();
    let again: Vec<String> = (0..50).map(|_| predicate(&mut b)).collect();
    assert_eq!(types, again);
    let mut distinct = types.clone();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() > 5, "{distinct:?}");
    let mut one = CommitShape::new(1, 10, 1, 7);
    assert!((0..50).all(|_| predicate(&mut one) == CommitShape::iri("rel_t0_org").as_str()));
}

#[test]
fn test_the_draws_are_marsaglias_xorshift64() {
    // Marsaglia, "Xorshift RNGs" (J. Stat. Soft. 8(14), 2003), xor64 with shifts 13, 7, 17 from
    // 88172645463325252 yields 8748534153485358512, then 3040900993826735515, then 3453997556048239312. The shape
    // mixes its seed with 0x9E3779B97F4A7C15, so the seed 88172645463325252 ^ 0x9E3779B97F4A7C15 =
    // 11461161000241530449 starts that stream; u64::MAX types and hubs leave each draw below them as it is.
    let mut shape = CommitShape::new(2, u64::MAX, u64::MAX, 11_461_161_000_241_530_449);
    let commit = inserts(shape.next_commit().changes());
    assert_eq!(
        commit[0].0,
        CommitShape::iri("rel_t8748534153485358512_org").as_str()
    );
    assert_eq!(
        commit[0].2,
        Term::from(CommitShape::hub(3_040_900_993_826_735_515))
    );
    assert_eq!(
        commit[2].2,
        Term::from(CommitShape::hub(3_453_997_556_048_239_312))
    );
}

#[test]
fn test_a_seed_of_zero_draws_as_a_seed_of_one() {
    let draws = |seed| {
        let mut shape = CommitShape::new(3, 1_000, 10, seed);
        (0..5)
            .map(|_| inserts(shape.next_commit().changes()))
            .collect::<Vec<_>>()
    };
    assert_eq!(draws(0), draws(1));
    assert_ne!(draws(1), draws(2));
}

#[test]
fn test_one_hub_takes_every_link_and_no_relations_write_nothing() {
    let mut one = CommitShape::new(4, 1, 10, 7);
    let commit = inserts(one.next_commit().changes());
    assert_eq!(commit.len(), 8);
    assert!(
        commit
            .chunks(2)
            .all(|pair| pair[0].2 == Term::from(CommitShape::hub(0)))
    );
    let mut none = CommitShape::new(0, 1_000, 10, 7);
    assert_eq!(none.relations(), 0);
    assert!(none.next_commit().changes().is_empty());
    assert_eq!(one.relations(), 4);
}

#[test]
fn test_no_types_declare_the_hub_name_alone() {
    let changeset = CommitShape::declarations(0);
    let [Change::Declare(Declaration::Predicate(name))] = changeset.changes() else {
        panic!("{:?} is not the name alone", changeset.changes());
    };
    assert_eq!(name.predicate(), &CommitShape::iri("name"));
    assert_eq!(name.tokenizers(), ["exact"]);
    assert!(CommitShape::new(1, 0, 1, 7).hubs().changes().is_empty());
}
