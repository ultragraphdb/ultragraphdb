/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Recovery (7.9): the log's rounds above the store's applied version replayed into the store, the session table
//! with them, a build's swap built again, a store ahead of the log or below a trimmed one refused.

use futures::executor::block_on;
use oxrdf::Term;
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_traits_log::Position;
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_commit::{
    Pipeline, RecoveryErrorEnum, Replay, Reply, RoundBounds, Sessions,
};
use ultragraphdb_transactions_txn::Prepared;

use crate::utils::fixture::{
    Applier, Log, NS, applier, applier_over, begin, ex, held, insert, insert_literal, prepare,
    prepare_in, reopen_wal, setup, sim_store, uid_of, wal, write,
};

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// Commits `changes` through `pipe` under `ticket`, in a round of their own: the reply.
fn commit(pipe: &mut Pipe<'_>, applier: &Applier, ticket: u64, changes: Vec<Change>) -> Reply {
    let mut txn = begin(applier);
    write(&mut txn, changes);
    pipe.commit(ticket, prepare(txn), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    pipe.replies_mut().pop().expect("answered")
}

/// Replays `log` into `applier`'s store from position `from`.
fn replay(
    applier: &Applier,
    log: &mut Log,
    from: u64,
) -> Result<Replay, ultragraphdb_transactions_commit::RecoveryError> {
    block_on(Replay::run(
        applier,
        log,
        Position::new(from),
        Sessions::DEFAULT_CAPACITY,
    ))
}

#[test]
fn test_rounds_the_store_lost_are_replayed() {
    let store = sim_store();
    let applier = applier_over(store.clone());
    let (storage, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).unwrap();
    for i in 1..=10u64 {
        commit(
            &mut pipe,
            &applier,
            i,
            vec![insert(&format!("s{i}"), "knows", "b")],
        );
        if i == 7 {
            applier.quads().store().sync().unwrap();
        }
    }
    let tail = applier.quads().lists().applied_ts();
    drop(pipe);
    drop(applier);
    store.power_loss();
    store.recover();
    let applier = applier_over(store);
    let lost = applier.quads().lists().applied_ts();
    assert!(lost < tail, "rounds 8 to 10 were never synced to the store");
    let (mut log, _) = reopen_wal(&storage);
    let replayed = replay(&applier, &mut log, 1).unwrap();
    assert_eq!(applier.quads().lists().applied_ts(), tail);
    assert_eq!(replayed.applied(), tail);
    assert!(
        replayed.rounds() >= 1 && replayed.rounds() <= 3,
        "{}",
        replayed.rounds()
    );
    for i in 1..=10 {
        assert_eq!(held(&applier, &format!("s{i}"), "knows").len(), 1, "s{i}");
    }
    // Replaying again replays nothing: the store holds the log's last round.
    let (mut log, _) = reopen_wal(&storage);
    assert_eq!(replay(&applier, &mut log, 1).unwrap().rounds(), 0);
}

#[test]
fn test_a_store_ahead_of_the_log_refuses_to_start() {
    let applier = applier();
    let (_storage, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).unwrap();
    commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")]);
    let applied = applier.quads().lists().applied_ts();
    drop(pipe);
    // Another log, holding none of the store's rounds.
    let (_other, mut log) = wal();
    let refused = replay(&applier, &mut log, 1).unwrap_err();
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::StoreNotInLog { version: applied }
    );
    assert!(
        refused.to_string().contains(&applied.to_string()),
        "{refused}"
    );
    // A store written outside the log, below its first round.
    let outside = crate::utils::fixture::applier();
    setup(&outside, vec![insert("x", "knows", "y")]);
    let version = outside.quads().lists().applied_ts();
    let (_storage, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&outside, log, unbounded()).unwrap();
    commit(&mut pipe, &outside, 1, vec![insert("a", "knows", "b")]);
    let mut log = pipe.into_log();
    let fresh = applier_over(sim_store());
    setup(&fresh, vec![insert("x", "knows", "y")]);
    assert_eq!(
        replay(&fresh, &mut log, 1).unwrap_err().0,
        RecoveryErrorEnum::StoreNotInLog { version }
    );
}

#[test]
fn test_an_empty_store_below_a_trimmed_log_refuses_to_start() {
    let applier = applier();
    let (_storage, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).unwrap();
    commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")]);
    commit(&mut pipe, &applier, 2, vec![insert("c", "knows", "b")]);
    let mut log = pipe.into_log();
    let empty = applier_over(sim_store());
    assert_eq!(
        replay(&empty, &mut log, 2).unwrap_err().0,
        RecoveryErrorEnum::Gap { checkpoint: 1 }
    );
}

/// The term index of `<p>`, its declaration with the `term` tokenizer.
fn term_index(p: &str) -> Change {
    Change::Declare(Declaration::Predicate(
        PredicateDeclaration::new(ex(p)).with_tokenizers(vec!["term".to_string()]),
    ))
}

/// The subjects the `term` index of `<predicate>` lists under `word`.
fn indexed(applier: &Applier, predicate: &str, word: &str) -> Vec<u64> {
    use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(applier, predicate),
        tokenizer: 0x01,
        token: word.as_bytes().to_vec(),
    }
    .to_bytes();
    applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())
        .unwrap()
        .iter()
        .filter_map(|(o, _)| o.as_uid())
        .collect()
}

/// Every kind of round replayed into an empty store gives the store the first apply left: transactions, a session's
/// registration and its committed request with its stored reply, a build's swap built again, a drop, and a swap whose
/// build a drop cancelled, discarded.
#[test]
fn test_every_kind_of_round_replays_into_an_empty_store() {
    let original = applier();
    let (_storage, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded()).unwrap();
    commit(
        &mut pipe,
        &original,
        1,
        vec![insert_literal("a", "name", "big apple")],
    );
    pipe.register(2, 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let session = pipe.replies_mut().pop().unwrap().outcome().clone().unwrap();
    let mut txn = begin(&original);
    write(&mut txn, vec![insert("s", "knows", "b")]);
    pipe.commit(3, prepare_in(txn, session, 1), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let committed = pipe.replies_mut().pop().unwrap().outcome().clone().unwrap();
    // A build, swapped in.
    commit(&mut pipe, &original, 4, vec![term_index("name")]);
    original.executor().run_until_idle();
    pipe.poll_builds(0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    assert!(!original.rebuilding());
    // A build cancelled by a drop: its swap discarded.
    commit(
        &mut pipe,
        &original,
        5,
        vec![insert_literal("c", "label", "pear tree")],
    );
    commit(&mut pipe, &original, 6, vec![term_index("label")]);
    original.executor().run_until_idle();
    let drop =
        Prepared::drop_predicate(&original, NS, ex("label").as_ref(), Default::default()).unwrap();
    pipe.commit(7, drop, 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    pipe.poll_builds(0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    assert!(!original.rebuilding());
    let applied = original.quads().lists().applied_ts();
    let mut log = pipe.into_log();
    let replica = applier_over(sim_store());
    let replayed = replay(&replica, &mut log, 1).unwrap();
    assert_eq!(replica.quads().lists().applied_ts(), applied);
    assert!(replayed.rounds() >= 8);
    assert_eq!(held(&replica, "s", "knows"), held(&original, "s", "knows"));
    assert_eq!(
        indexed(&replica, "name", "apple"),
        vec![uid_of(&replica, "a")]
    );
    assert_eq!(
        indexed(&replica, "name", "apple"),
        indexed(&original, "name", "apple")
    );
    let name = |a: &Applier| a.catalog(NS).unwrap().predicate(uid_of(a, "name")).cloned();
    assert_eq!(name(&replica), name(&original));
    assert!(held(&replica, "c", "label").is_empty(), "dropped");
    assert!(
        replica
            .catalog(NS)
            .unwrap()
            .predicate(uid_of(&replica, "label"))
            .is_none()
    );
    // The session table, with the stored reply.
    let table = Sessions::load(replica.quads().store()).unwrap();
    assert_eq!(table.last(session), Some((1, committed)));
    let _: Option<Term> = None;
}
