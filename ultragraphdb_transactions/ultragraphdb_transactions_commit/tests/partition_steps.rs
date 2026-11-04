/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The single-store partitions scenarios and the write-ahead-log scenarios replaying a partition's references and
//! registrations: a read-write transaction bound to one partition of a placement map over one store, whose second
//! partition is the map's alone, and the pipeline over the write-ahead log on simulated storage, its log replayed into
//! an empty store. The placement map and the bucket derivation are the oracle of every owner.

use cucumber::{then, when};
use oxrdf::{GraphName, Literal, NamedOrBlankNode, Term};
use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, Pipeline, Reply};
use ultragraphdb_transactions_txn::{Prepared, TxnError};

use crate::utils::fixture::{
    Applier, applier, applier_over, begin, changeset, ex, held, insert, insert_literal, member_at,
    open_wal, prepare, read, registered, setup, sim_store, version, write,
};
use crate::{
    Pipe, Steps, insert_if_absent, outcomes, owned_by, owning_value, partition, replay_into,
    statements_of, two_partitions, unbounded, unique_id,
};

/// One partition owning every bucket, so the namespace's whole run.
fn one_partition() -> TestResult<PlacementMap> {
    PlacementMap::even(1, ultragraphdb_cluster_placement::NodeId::new(1)).context("the placement")
}

/// Commits `prepared` under ticket 1 through a pipeline over a fresh log, its commit-time operations claiming for
/// `partition` of `placement`, every round synced: the replies.
async fn commit_bound(
    applier: &Applier,
    prepared: Prepared,
    partition: PartitionId,
    placement: &PlacementMap,
) -> TestResult<Vec<Reply>> {
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(applier, log, unbounded())
        .context("it opens")?
        .bound(partition, placement);
    pipe.commit(1, prepared, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    Ok(std::mem::take(pipe.replies_mut()))
}

/// The subjects of `knows` pointing at `<object>` that `txn` reads.
fn subjects_of(
    txn: &mut crate::utils::fixture::Txn<'_>,
    object: &str,
) -> Result<Vec<NamedOrBlankNode>, TxnError> {
    let mut out = Vec::new();
    let (p, o) = (ex("knows"), ex(object));
    txn.subjects_into(
        GraphName::DefaultGraph.as_ref(),
        p.as_ref(),
        o.as_ref().into(),
        0,
        &mut out,
    )?;
    Ok(out)
}

#[when(
    "a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition 2 owns"
)]
fn second_partition_after_first(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (first, second) = (owned_by(&placement, 1)?, owned_by(&placement, 2)?);
    // Bound to the partition of the first subject it writes.
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    let taken = txn
        .write(&changeset(vec![insert(&first, "knows", "b")]), 0)
        .is_ok();
    w.refusal = txn
        .write(&changeset(vec![insert(&second, "knows", "b")]), 0)
        .err();
    w.checks = vec![
        taken,
        txn.is_ended(),
        txn.commit(1, 1, &[], 0).err() == Some(TxnError::ended()),
    ];
    w.held = [
        held(&applier, &first, "knows")?,
        held(&applier, &second, "knows")?,
    ]
    .concat();
    Ok(())
}

#[then("the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing")]
fn aborts_holding_nothing(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(TxnError::cross_partition(1, 2)));
    assert!(w.checks.iter().all(|c| *c), "{:?}", w.checks);
    assert!(w.held.is_empty(), "{:?}", w.held);
    Ok(())
}

#[when(
    "`alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction reads `on-call` of `alice` and then of `bob`"
)]
fn write_skew_across_partitions(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (alice, bob) = (owned_by(&placement, 1)?, owned_by(&placement, 2)?);
    let on_call = |who: &str| insert_literal(who, "on-call", "true");
    let _version: u64 = setup(&applier, vec![on_call(&alice), on_call(&bob)])?;
    let watermark = applier.quads().lists().applied_ts();
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    let first = read(&mut txn, &alice, "on-call")?;
    let mut out = Vec::new();
    let (b, p) = (ex(&bob), ex("on-call"));
    w.refusal = txn
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            b.as_ref().into(),
            p.as_ref(),
            0,
            &mut out,
        )
        .err();
    w.checks = vec![
        first == vec![Term::from(Literal::new_simple_literal("true"))],
        txn.is_ended(),
        txn.commit(1, 1, &[], 0).err() == Some(TxnError::ended()),
        applier.quads().lists().applied_ts() == watermark,
    ];
    Ok(())
}

#[then(
    "the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes nothing"
)]
fn second_read_aborts(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(TxnError::cross_partition(1, 2)));
    assert!(w.checks.iter().all(|c| *c), "{:?}", w.checks);
    Ok(())
}

#[when(
    "a namespace's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound to partition 1 reads the subjects of `knows` pointing at `<b>`"
)]
fn reverse_read_spread(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (x, b) = (owned_by(&placement, 1)?, owned_by(&placement, 2)?);
    let _version: u64 = setup(&applier, vec![insert(&x, "knows", &b)])?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    w.refusal = subjects_of(&mut txn, &b).err();
    w.checks = vec![txn.is_ended()];
    Ok(())
}

#[then("the transaction aborts with `CrossPartition` naming partitions 1 and 2")]
fn aborts_cross_partition(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(TxnError::cross_partition(1, 2)));
    assert!(w.checks.iter().all(|c| *c), "{:?}", w.checks);
    Ok(())
}

#[when(
    "one partition owns the namespace's whole run, and a read-write transaction reads the subjects of `knows` pointing at `<b>` and then writes `<x> <seen> true`"
)]
async fn own_reverse_entries(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = one_partition()?;
    let _version: u64 = setup(&applier, vec![insert("x", "knows", "b")])?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    w.names = subjects_of(&mut txn, "b")?
        .iter()
        .map(ToString::to_string)
        .collect();
    write(
        &mut txn,
        vec![Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("x").into(),
            predicate: ex("seen"),
            object: Literal::from(true).into(),
        }],
    )?;
    w.replies = commit_bound(&applier, prepare(txn)?, partition(1)?, &placement).await?;
    w.held = held(&applier, "x", "seen")?;
    Ok(())
}

#[then(
    "the read answers from the partition's own reverse entries at its latest state, and the transaction commits"
)]
fn own_reverse_answered(w: &mut Steps) -> TestResult {
    assert_eq!(w.names, vec![ex("x").to_string()]);
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(1, 0)))]);
    assert_eq!(w.held, vec![Term::from(Literal::from(true))]);
    Ok(())
}

#[when(
    "a namespace's run spans partitions 1 and 2, and a read-write transaction's first operation looks up the `term` index of `name`"
)]
fn first_lookup_spread(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let declared = PredicateDeclaration::new(ex("name")).with_tokenizers(vec!["term".into()]);
    let _version: u64 = setup(
        &applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    let own = owned_by(&placement, 1)?;
    let _named: u64 = setup(&applier, vec![insert_literal(&own, "name", "ada")])?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    let (p, v) = (ex("name"), Literal::new_simple_literal("ada"));
    let mut out = Vec::new();
    w.refusal = txn
        .lookup_into(
            GraphName::DefaultGraph.as_ref(),
            p.as_ref(),
            "term",
            v.as_ref().into(),
            0,
            &mut out,
        )
        .err();
    w.checks = vec![
        out.is_empty(),
        txn.is_ended(),
        txn.commit(1, 1, &[], 0).err() == Some(TxnError::ended()),
    ];
    w.held = Vec::new();
    Ok(())
}

#[when(
    "`epi.id` is declared unique, the bucket of `\"x\"` under `epi.id` belongs to partition 2, and a transaction bound to partition 1 insert-if-absents a node with `epi.id` `\"x\"`"
)]
async fn unique_claim_of_partition_2(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let placement = owning_value(&applier, "x")?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    txn.operate(insert_if_absent("x"), 0).context("asked")?;
    w.replies = commit_bound(&applier, prepare(txn)?, partition(1)?, &placement).await?;
    w.statements = vec![statements_of(&applier, "epi.id")?];
    Ok(())
}

#[then(
    "the transaction aborts with `CrossPartition` naming partitions 1 and 2, and no node holds `epi.id` `\"x\"`"
)]
fn unique_claim_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Err(CommitError::cross_partition(1, 2)))]
    );
    assert_eq!(w.statements, vec![0], "no statement of `epi.id` is held");
    Ok(())
}

#[when(
    "`epi.id` is declared unique, the bucket of `\"x\"` under `epi.id` belongs to partition 2, and a transaction bound to partition 1 writes `<a> <epi.id> \"x\"`"
)]
fn unique_write_of_partition_2(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let placement = owning_value(&applier, "x")?;
    // `<a>` is partition 1's, so only the value reaches partition 2.
    let a = owned_by(&placement, 1)?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    w.refusal = txn
        .write(&changeset(vec![insert_literal(&a, "epi.id", "x")]), 0)
        .err();
    w.checks = vec![txn.is_ended()];
    w.held = held(&applier, &a, "epi.id")?;
    Ok(())
}

#[then(
    "the transaction aborts with `CrossPartition` naming partitions 1 and 2, and `<a>` holds no `epi.id`"
)]
fn unique_write_refused(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(TxnError::cross_partition(1, 2)));
    assert!(w.checks.iter().all(|c| *c), "{:?}", w.checks);
    assert!(w.held.is_empty(), "{:?}", w.held);
    Ok(())
}

/// The delete by pattern of `<subject>`'s `predicate`.
fn delete_pattern(subject: &str, predicate: &str) -> Change {
    Change::DeletePattern {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
    }
}

#[when(
    "a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2"
)]
fn delete_pattern_of_partition_2(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let b = owned_by(&placement, 2)?;
    let _version: u64 = setup(
        &applier,
        vec![insert(&b, "knows", "c"), insert(&b, "knows", "d")],
    )?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    w.refusal = txn
        .write(&changeset(vec![delete_pattern(&b, "knows")]), 0)
        .err();
    w.checks = vec![txn.is_ended()];
    w.held = held(&applier, &b, "knows")?;
    Ok(())
}

#[then(
    "the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still held"
)]
fn delete_pattern_refused(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusal, Some(TxnError::cross_partition(1, 2)));
    assert!(w.checks.iter().all(|c| *c), "{:?}", w.checks);
    let mut held = w.held.clone();
    held.sort_by_key(ToString::to_string);
    assert_eq!(held, vec![Term::from(ex("c")), Term::from(ex("d"))]);
    Ok(())
}

#[when(
    "`<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes the pattern `<b>`, `name`"
)]
async fn delete_pattern_at_home(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let placement = two_partitions()?;
    let (a, b) = (owned_by(&placement, 1)?, owned_by(&placement, 2)?);
    let _version: u64 = setup(
        &applier,
        vec![insert(&a, "knows", &b), insert_literal(&b, "name", "B")],
    )?;
    let mut txn = begin(&applier)?.bind(partition(2)?, &placement);
    write(&mut txn, vec![delete_pattern(&b, "name")])?;
    w.replies = commit_bound(&applier, prepare(txn)?, partition(2)?, &placement).await?;
    w.terms = vec![held(&applier, &b, "name")?, held(&applier, &a, "knows")?];
    w.names = vec![b];
    Ok(())
}

#[then(
    "it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held"
)]
fn delete_pattern_at_home_commits(w: &mut Steps) -> TestResult {
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(1, 0)))]);
    let b = w.names.first().context("`<b>`'s name")?;
    assert_eq!(w.terms, vec![Vec::new(), vec![Term::from(ex(b))]]);
    Ok(())
}

#[when(
    "a round holding a first reference to an entity another partition owns, with its registration mark and the copied ID-to-IRI entry, is applied, the store is discarded, and the round's record is replayed into an empty store"
)]
async fn replay_reference(w: &mut Steps) -> TestResult {
    let original = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded()).context("it opens")?;
    let b = crate::utils::proposal_double::id(40)?;
    let group = BucketGroup::new(7).context("the group")?;
    let generation = std::num::NonZeroU64::new(4).context("a generation")?;
    let mut txn = begin(&original)?;
    // The home's reply: `<b>`'s ID and the generation of `(b, 7)`, with the latest close delivered to the home.
    txn.adopt(ex("b").as_str(), b, Epoch::ZERO, 0)
        .context("adopted")?;
    txn.mark(b, group, generation, Epoch::ZERO, 0)
        .context("marked")?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    let at = pipe.watermark();
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let _replayed = replay_into(&replica, &mut written).await?;
    let entry = |applier: &Applier| applier.quads().id_of(0, ex("b").as_str());
    w.checks = vec![
        original.quads().store().contents() == replica.quads().store().contents(),
        replica.quads().mark(0, b, group, at)? == Some(generation),
        original.quads().mark(0, b, group, at)? == Some(generation),
        entry(&replica)? == Some(b),
        entry(&original)? == Some(b),
    ];
    Ok(())
}

#[then(
    "the store holds exactly the keys and values it held after the first apply, the mark's generation and the copied entry included"
)]
fn replay_reference_same_keys(w: &mut Steps) -> TestResult {
    assert_eq!(w.checks, vec![true; 5]);
    Ok(())
}

#[when(
    "a round holding only two registrations a home accepted is applied, the store is discarded, and the record is replayed"
)]
async fn replay_registrations(w: &mut Steps) -> TestResult {
    let original = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded()).context("it opens")?;
    // A commit's round in flight, so the two registrations wait for a round holding only them.
    let mut txn = begin(&original)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0).context("appended")?;
    let (o, o_executed, o_record) = registered(&original, "o", 7, 1)?;
    let (p, p_executed, p_record) = registered(&original, "p", 9, 2)?;
    pipe.registration(2, o_executed, o_record, 0)
        .context("handed over")?;
    pipe.registration(3, p_executed, p_record, 0)
        .context("handed over")?;
    pipe.sync(0).await.context("synced")?;
    pipe.sync(0).await.context("synced")?;
    let replies = std::mem::take(pipe.replies_mut());
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let _replayed = replay_into(&replica, &mut written).await?;
    let mut members = Vec::new();
    for (entity, group, ticket) in [(o, 7, 2), (p, 9, 3)] {
        let at = replies
            .iter()
            .find(|r| r.ticket() == ticket)
            .and_then(|r| r.outcome().clone().ok())
            .context("the registration committed")?;
        members.push((
            ticket,
            at,
            member_at(&replica, entity, group, at)?,
            member_at(&replica, entity, group, at - 1)?,
        ));
    }
    w.versions = members.iter().map(|(_, at, _, _)| *at).collect();
    w.checks = members
        .iter()
        .map(|(_, at, member, before)| {
            let epoch = at >> 22;
            member.is_some_and(|(_, e)| e == epoch) && before.is_none()
        })
        .collect();
    Ok(())
}

#[then(
    "the referrer set holds both members, each at the commit version the record lists for its registration"
)]
fn replay_registrations_members(w: &mut Steps) -> TestResult {
    // The round after the commit's: epoch 1, sequences 2 and 4.
    assert_eq!(w.versions, vec![version(1, 2), version(1, 4)]);
    assert_eq!(w.checks, vec![true, true]);
    Ok(())
}
