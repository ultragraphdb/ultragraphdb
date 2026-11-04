/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::cluster::{Cluster, begin, changeset, group, insert, pid};

/// The references `changes` written at partition `id` would register, in order.
fn references_of(
    cluster: &Cluster,
    id: u32,
    changes: Vec<ultragraphdb_core_mapping::Change>,
) -> TestResult<Vec<ultragraphdb_cluster_partition::Reference>> {
    let part = cluster.part(id)?;
    let mut txn = begin(part)?;
    let mut out = Vec::new();
    part.references(&mut txn, &changeset(changes), cluster.closed(), 0, &mut out)?;
    let _buffers = txn.abort();
    Ok(out)
}

/// An edge to an entity another partition owns, unknown here, registers it by its IRI from the subject's group,
/// having seen no generation; a predicate this partition owns registers nothing.
#[test]
fn test_a_new_foreign_object_registers_by_its_iri_from_its_subjects_group() -> TestResult {
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let refs = references_of(&cluster, 1, vec![insert(&a, &knows, &b)])?;
    assert_eq!(refs.len(), 1, "{refs:?}");
    let reference = refs.first().context("one reference")?;
    assert_eq!(reference.home(), pid(2)?);
    assert_eq!(reference.iri(), crate::utils::cluster::ex(&b).as_str());
    assert_eq!(
        (reference.entity(), reference.group(), reference.named()),
        (None, Some(group(&a)?), 0)
    );
    Ok(())
}

/// A predicate another partition owns registers with no group; with its subject and object local, nothing else does.
#[test]
fn test_a_foreign_predicate_registers_with_no_group() -> TestResult {
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let c = cluster.name_in(1, "c")?;
    let knows = cluster.name_in(2, "knows")?;
    let refs = references_of(&cluster, 1, vec![insert(&a, &knows, &c)])?;
    assert_eq!(refs.len(), 1, "{refs:?}");
    let reference = refs.first().context("one reference")?;
    assert_eq!((reference.home(), reference.group()), (pid(2)?, None));
    assert_eq!(reference.iri(), crate::utils::cluster::ex(&knows).as_str());
    Ok(())
}

/// Two edges from one group to one foreign entity register once; references come sorted by home.
#[test]
fn test_references_are_registered_once_each_sorted_by_home() -> TestResult {
    let cluster = Cluster::even(3)?;
    let a = cluster.name_in(1, "a")?;
    let knows = cluster.name_in(1, "knows")?;
    let likes = cluster.name_in(1, "likes")?;
    let in_three = cluster.name_in(3, "c")?;
    let in_two = cluster.name_in(2, "b")?;
    let refs = references_of(
        &cluster,
        1,
        vec![
            insert(&a, &knows, &in_three),
            insert(&a, &knows, &in_two),
            insert(&a, &likes, &in_two),
        ],
    )?;
    let homes: Vec<u32> = refs.iter().map(|r| r.home().get()).collect();
    assert_eq!(homes, vec![2, 3], "{refs:?}");
    Ok(())
}

/// Once a reference from a group is committed with its mark, a later edge from that group registers nothing, and one
/// from another group of the partition registers by the ID the partition now holds.
#[test]
fn test_a_marked_reference_registers_nothing_and_another_group_registers_by_id() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let _version = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    let same_group = cluster.name_where("s", |name| {
        Ok(cluster.owner(name)? == 1 && group(name)? == group(&a)?)
    })?;
    assert!(references_of(&cluster, 1, vec![insert(&same_group, &knows, &b)])?.is_empty());
    let other_group = cluster.name_where("t", |name| {
        Ok(cluster.owner(name)? == 1 && group(name)? != group(&a)?)
    })?;
    let refs = references_of(&cluster, 1, vec![insert(&other_group, &knows, &b)])?;
    let held = cluster
        .id_at(1, &b)?
        .context("partition 1 copied b's entries")?;
    let reference = refs.first().context("one reference")?;
    assert_eq!(
        (reference.entity(), reference.group(), reference.named()),
        (Some(held), Some(group(&other_group)?), 0)
    );
    Ok(())
}

/// Adopting an answer holds the transaction's commit until the close the home named: its floor is one above it.
#[test]
fn test_an_adopted_answer_raises_the_floor_to_one_above_its_close() -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let refs = references_of(&cluster, 1, vec![insert(&a, &knows, &b)])?;
    let reference = refs.first().context("one reference")?.clone();
    let answer = cluster.register(&reference)?;
    let part = cluster.part(1)?;
    let mut txn = begin(part)?;
    part.adopt(&mut txn, &reference, &answer, 0)?;
    assert_eq!(txn.epoch_floor(), Some(answer.close().next()?));
    let _buffers = txn.abort();
    Ok(())
}

/// A namespace's first write names `rdf:type`, which its catalog declares: a partition not holding it gets its ID from
/// its home, so that first write registers it with no group; the same partition's next write registers nothing more.
#[test]
fn test_a_first_write_gets_rdf_type_at_its_home() -> TestResult {
    use oxrdf::vocab::rdf;
    use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
    use ultragraphdb_store_quads::iri_bucket;
    let placement = PlacementMap::even(2, NodeId::new(1))?;
    let bucket = iri_bucket(crate::utils::cluster::NS, rdf::TYPE.as_str()).context("its bucket")?;
    let home = placement
        .partition_of(bucket, crate::utils::cluster::epoch(1)?)
        .get();
    let other = if home == 1 { 2 } else { 1 };
    let mut cluster = Cluster::bare(&placement)?;
    let refs = references_of(&cluster, other, Vec::new())?;
    let reference = refs.first().context("rdf:type's reference")?;
    assert_eq!(
        (refs.len(), reference.home(), reference.group()),
        (1, pid(home)?, None)
    );
    assert_eq!(reference.iri(), rdf::TYPE.as_str());
    let _version = cluster.write(other, Vec::new())?.version()?;
    assert!(references_of(&cluster, other, Vec::new())?.is_empty());
    Ok(())
}

/// A read of a predicate homed elsewhere gets its ID from its home with no group; one held, or the partition's own,
/// registers nothing, and a graph is treated as a predicate is.
#[test]
fn test_a_read_of_a_foreign_predicate_gets_its_id_first() -> TestResult {
    use crate::utils::cluster::{NS, ex};
    use oxrdf::GraphName;
    let mut cluster = Cluster::even(2)?;
    let foreign = cluster.name_in(2, "knows")?;
    let own = cluster.name_in(1, "likes")?;
    let closed = cluster.closed();
    let mut refs = Vec::new();
    let part = cluster.part(1)?;
    part.read_references(NS, &GraphName::DefaultGraph, &ex(&own), closed, &mut refs)?;
    assert!(refs.is_empty(), "{refs:?}");
    part.read_references(
        NS,
        &GraphName::DefaultGraph,
        &ex(&foreign),
        closed,
        &mut refs,
    )?;
    part.read_references(NS, &ex(&foreign).into(), &ex(&own), closed, &mut refs)?;
    assert_eq!(refs.len(), 2, "a predicate and a graph: {refs:?}");
    let home = pid(2)?;
    assert!(refs.iter().all(|r| r.home() == home && r.group().is_none()));
    let reference = refs.first().context("the reference")?.clone();
    let answer = cluster.register(&reference)?;
    let _version = cluster
        .write_answered(1, Vec::new(), vec![(reference, answer)])?
        .version()?;
    let mut after = Vec::new();
    cluster.part(1)?.read_references(
        NS,
        &GraphName::DefaultGraph,
        &ex(&foreign),
        cluster.closed(),
        &mut after,
    )?;
    assert!(after.is_empty(), "{after:?}");
    Ok(())
}

/// The node a claim makes takes its bucket at the commit step, so a value or a link naming a node homed elsewhere is
/// refused cross-partition; a compare-and-set's new value, an edge from a known subject, registers as an insert does.
#[test]
fn test_an_operation_naming_a_foreign_node_registers_or_is_refused() -> TestResult {
    use crate::utils::cluster::{NS, ex};
    use oxrdf::GraphName;
    use ultragraphdb_cluster_partition::PartitionError;
    use ultragraphdb_store_apply::{Operation, OperationLink};
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let p = cluster.name_in(1, "p")?;
    let claim = |value: oxrdf::Term, links: Vec<OperationLink>| Operation::UpsertAndLink {
        graph: GraphName::DefaultGraph,
        predicate: ex(&p),
        value,
        properties: Vec::new(),
        links,
    };
    let closed = cluster.closed();
    let part = cluster.part(1)?;
    let mut txn = begin(part)?;
    let mut refs = Vec::new();
    let refused = PartitionError::cross_partition(1, 2);
    let by_value = claim(ex(&b).into(), Vec::new());
    assert_eq!(
        part.operation_references(&mut txn, NS, &by_value, closed, 0, &mut refs),
        Err(refused.clone())
    );
    let by_link = claim(
        oxrdf::Literal::new_simple_literal("v").into(),
        vec![OperationLink::from_node(ex(&p), ex(&b).into())],
    );
    assert_eq!(
        part.operation_references(&mut txn, NS, &by_link, closed, 0, &mut refs),
        Err(refused)
    );
    let set = Operation::CompareAndSet {
        graph: GraphName::DefaultGraph,
        subject: ex(&a).into(),
        predicate: ex(&p),
        expected: oxrdf::Literal::new_simple_literal("old").into(),
        new: ex(&b).into(),
    };
    part.operation_references(&mut txn, NS, &set, closed, 0, &mut refs)?;
    let reference = refs.first().context("the new value's reference")?;
    assert_eq!((refs.len(), reference.group()), (1, Some(group(&a)?)));
    let _buffers = txn.abort();
    Ok(())
}

/// An edge from a new blank node registers from the bucket group the node's ID takes, the one the transaction's write
/// then gives it: the label resolves once.
#[test]
fn test_a_new_blank_subject_registers_from_the_group_its_id_takes() -> TestResult {
    use crate::utils::cluster::ex;
    use oxrdf::{BlankNode, GraphName};
    use ultragraphdb_core_ids::BucketGroup;
    use ultragraphdb_core_mapping::Change;
    let cluster = Cluster::even(2)?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let label = BlankNode::new("n")?;
    let change = Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: label.clone().into(),
        predicate: ex(&knows),
        object: ex(&b).into(),
    };
    let part = cluster.part(1)?;
    let mut txn = begin(part)?;
    let mut refs = Vec::new();
    part.references(
        &mut txn,
        &changeset(vec![change]),
        cluster.closed(),
        0,
        &mut refs,
    )?;
    let id = txn.node_id(label.as_ref().into(), 0)?;
    assert_eq!(
        refs.first().and_then(|r| r.group()),
        Some(BucketGroup::of(id.bucket()))
    );
    part.route(id.bucket(), cluster.closed())?;
    let _buffers = txn.abort();
    Ok(())
}

/// A literal object and a predicate the partition owns name nothing homed elsewhere: the write registers nothing.
#[test]
fn test_a_local_statement_registers_nothing() -> TestResult {
    use crate::utils::cluster::insert_literal;
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let p = cluster.name_in(1, "p")?;
    let written = cluster.write(1, vec![insert_literal(&a, &p, "v")])?;
    assert!(written.references.is_empty(), "{:?}", written.references);
    let _version = written.version()?;
    Ok(())
}

/// What a reference left survives a restart: the partition finds the entity's copied ID and its mark in its own store,
/// and a later edge from the group registers nothing, as of the commit's epoch and after.
#[test]
fn test_the_mark_and_the_copied_entries_survive_a_restart() -> TestResult {
    use crate::utils::cluster::{NS, as_of};
    use ultragraphdb_core_ids::Version;
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let version = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    let at = Version::from_u64(version).epoch();
    cluster.restart(1)?;
    let held = cluster.id_at(1, &b)?.context("the copied entry")?;
    let quads = cluster.part(1)?.applier().quads();
    let read_at = as_of(at).min(quads.lists().applied_ts());
    assert!(quads.mark(NS, held, group(&a)?, read_at)?.is_some());
    assert!(references_of(&cluster, 1, vec![insert(&a, &knows, &b)])?.is_empty());
    assert!(
        cluster
            .contents(1)?
            .keys()
            .keys()
            .any(|k| k.windows(16).any(|w| w == held.to_be_bytes())),
        "the store holds keys naming the entity"
    );
    Ok(())
}

/// A renewal committed between a reply and its commit conflicts with that commit, retryable, which writes nothing: the
/// mark key is in the commit's read set (partitions, "A commit under a registration reply reads its mark").
#[test]
fn test_a_renewal_before_a_reply_commits_conflicts() -> TestResult {
    use crate::utils::cluster::{NS, Outcome, ex};
    use ultragraphdb_cluster_partition::Reference;
    use ultragraphdb_transactions_commit::CommitErrorEnum;
    let mut cluster = Cluster::even(2)?;
    let o = cluster.name_in(2, "o")?;
    let knows = cluster.name_in(1, "knows")?;
    let s = cluster.name_in(1, "s")?;
    let g = group(&s)?;
    let t = cluster.name_where("t", |n| Ok(cluster.owner(n)? == 1 && group(n)? == g))?;
    let of = |named| {
        Ok::<_, ultragraphdb_tool_bdd::TestError>(Reference::new(
            pid(2)?,
            NS,
            ex(&o).as_str().to_string(),
            None,
            Some(g),
            named,
        ))
    };
    let first = cluster.register(&of(0)?)?;
    let loser = cluster.prepare(1, &[(of(0)?, first)], vec![insert(&s, &knows, &o)])??;
    let renewed = cluster.register(&of(1)?)?;
    let winner = cluster.prepare(1, &[(of(1)?, renewed)], vec![insert(&t, &knows, &o)])??;
    let (won, lost) = cluster.commit_in_turn(1, winner, loser)?;
    assert!(matches!(won, Outcome::Committed(_)), "{won:?}");
    assert!(
        matches!(&lost, Outcome::Rejected(e) if matches!(e.0, CommitErrorEnum::Conflict { .. })),
        "{lost:?}"
    );
    Ok(())
}

/// A declaration reaches a partition not owning its predicate through the predicate's home: both hold one ID.
#[test]
fn test_a_declaration_reaches_every_partition_with_one_id() -> TestResult {
    use crate::utils::cluster::ex;
    use ultragraphdb_core_mapping::PredicateDeclaration;
    let mut cluster = Cluster::even(2)?;
    cluster.declare(
        "epi.id",
        PredicateDeclaration::new(ex("epi.id")).with_unique(true),
    )?;
    let one = cluster.id_at(1, "epi.id")?.context("partition 1's ID")?;
    assert_eq!(cluster.id_at(2, "epi.id")?, Some(one));
    Ok(())
}

/// Each change with the names it registers with no group, and the objects it registers from `a`'s group.
type ChangeCase = (ultragraphdb_core_mapping::Change, Vec<String>, Vec<String>);

/// The terms of partition 2 the change cases name.
struct Foreign {
    edge: String,
    key: String,
    value: String,
    graph: String,
    predicate: String,
}

impl Foreign {
    fn of(cluster: &Cluster) -> TestResult<Self> {
        Ok(Self {
            edge: cluster.name_in(2, "e")?,
            key: cluster.name_in(2, "k")?,
            value: cluster.name_in(2, "v")?,
            graph: cluster.name_in(2, "g")?,
            predicate: cluster.name_in(2, "q")?,
        })
    }

    /// The graph, by name.
    fn named(&self) -> oxrdf::GraphName {
        oxrdf::GraphName::NamedNode(crate::utils::cluster::ex(&self.graph))
    }
}

/// The owned names of `list`.
fn owned(list: &[&String]) -> Vec<String> {
    list.iter().map(|n| n.to_string()).collect()
}

/// The inserts and deletes of `a`, naming the local `local` and `f`'s terms.
fn statement_cases(a: &str, local: &str, f: &Foreign) -> Vec<ChangeCase> {
    use crate::utils::cluster::ex;
    use oxrdf::{GraphName, Literal};
    use ultragraphdb_core_mapping::Change;
    vec![
        (
            Change::InsertEdge {
                graph: GraphName::DefaultGraph,
                subject: ex(a).into(),
                predicate: ex(local),
                object: ex(&f.edge).into(),
                properties: vec![(ex(&f.key), ex(&f.value).into())],
            },
            owned(&[&f.key, &f.value]),
            owned(&[&f.edge]),
        ),
        (
            Change::Insert {
                graph: f.named(),
                subject: ex(a).into(),
                predicate: ex(&f.predicate),
                object: Literal::new_simple_literal("x").into(),
            },
            owned(&[&f.graph, &f.predicate]),
            Vec::new(),
        ),
        (
            Change::Delete {
                graph: f.named(),
                subject: ex(a).into(),
                predicate: ex(&f.predicate),
                object: ex(&f.value).into(),
            },
            owned(&[&f.graph, &f.predicate, &f.value]),
            Vec::new(),
        ),
        (
            Change::DeletePattern {
                graph: f.named(),
                subject: ex(a).into(),
                predicate: ex(&f.predicate),
            },
            owned(&[&f.graph, &f.predicate]),
            Vec::new(),
        ),
    ]
}

/// The changes of a statement named by its triple or its reifier.
fn property_cases(a: &str, local: &str, f: &Foreign) -> Vec<ChangeCase> {
    use crate::utils::cluster::ex;
    use oxrdf::{GraphName, Literal};
    use ultragraphdb_core_mapping::{Change, StatementRef};
    let triple = StatementRef::Triple {
        graph: GraphName::DefaultGraph,
        subject: ex(a).into(),
        predicate: ex(local),
        object: Literal::new_simple_literal("x").into(),
    };
    vec![
        (
            Change::SetProperty {
                statement: triple.clone(),
                predicate: ex(&f.key),
                value: ex(&f.value).into(),
            },
            owned(&[&f.key, &f.value]),
            Vec::new(),
        ),
        (
            Change::DeleteProperty {
                statement: triple,
                predicate: ex(&f.key),
                value: None,
            },
            owned(&[&f.key]),
            Vec::new(),
        ),
        (
            Change::DeleteStatement {
                graph: f.named(),
                reifier: ex(local),
            },
            owned(&[&f.graph]),
            Vec::new(),
        ),
    ]
}

/// Each kind of change names its terms: every IRI of a change homed elsewhere and unheld registers with no group, but an
/// inserted statement's object, which registers from its subject's group.
#[test]
fn test_every_change_kind_registers_what_it_names() -> TestResult {
    use crate::utils::cluster::ex;
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let local = cluster.name_in(1, "p")?;
    let foreign = Foreign::of(&cluster)?;
    let mut cases = statement_cases(&a, &local, &foreign);
    cases.extend(property_cases(&a, &local, &foreign));
    for (change, plain, from_group) in cases {
        let refs = references_of(&cluster, 1, vec![change.clone()])?;
        let of = |group: bool| -> Vec<String> {
            refs.iter()
                .filter(|r| r.group().is_some() == group)
                .map(|r| r.iri().to_string())
                .collect()
        };
        let iris = |list: &[String]| -> Vec<String> {
            let mut all: Vec<String> = list.iter().map(|n| ex(n).as_str().to_string()).collect();
            all.sort();
            all
        };
        assert_eq!(of(false), iris(&plain), "{change:?}");
        assert_eq!(of(true), iris(&from_group), "{change:?}");
    }
    Ok(())
}

/// A claim's graph, predicate and properties register with no group, and a counter names its graph and predicate.
#[test]
fn test_operations_register_what_they_name() -> TestResult {
    use crate::utils::cluster::{NS, ex};
    use oxrdf::{GraphName, Literal};
    use ultragraphdb_store_apply::{Operation, OperationLink};
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let own = cluster.name_in(1, "o")?;
    let (graph, predicate, key, linked) = (
        cluster.name_in(2, "g")?,
        cluster.name_in(2, "q")?,
        cluster.name_in(2, "k")?,
        cluster.name_in(2, "l")?,
    );
    let named = GraphName::NamedNode(ex(&graph));
    let value = || Literal::new_simple_literal("x");
    let cases = vec![
        (
            Operation::InsertIfAbsent {
                graph: named.clone(),
                predicate: ex(&predicate),
                value: value().into(),
                properties: vec![(ex(&key), value().into())],
            },
            vec![&graph, &predicate, &key],
        ),
        (
            Operation::UpsertAndLink {
                graph: GraphName::DefaultGraph,
                predicate: ex(&own),
                value: ex(&own).into(),
                properties: Vec::new(),
                links: vec![OperationLink::to_node(ex(&linked), ex(&own).into())],
            },
            vec![&linked],
        ),
        (
            Operation::CounterAdd {
                graph: named,
                subject: ex(&a).into(),
                predicate: ex(&predicate),
                delta: 1,
            },
            vec![&graph, &predicate],
        ),
    ];
    let part = cluster.part(1)?;
    for (operation, expected) in cases {
        let mut txn = begin(part)?;
        let mut refs = Vec::new();
        part.operation_references(&mut txn, NS, &operation, cluster.closed(), 0, &mut refs)?;
        let found: Vec<String> = refs.iter().map(|r| r.iri().to_string()).collect();
        let mut iris: Vec<String> = expected
            .iter()
            .map(|n| ex(n).as_str().to_string())
            .collect();
        iris.sort();
        assert_eq!(found, iris, "{operation:?}");
        assert!(refs.iter().all(|r| r.group().is_none()), "{refs:?}");
        let _buffers = txn.abort();
    }
    Ok(())
}
