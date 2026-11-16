/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step deciding real transactions, executed on a changeset applier over the simulated store, the driver
//! applying each synced round with `apply_round`.

use oxrdf::{Literal, Term};
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, RoundBounds};

use crate::utils::fixture::{
    Driver, applier, begin, begin_with, delete_literal, held, insert, insert_literal, prepare,
    read, setup, write,
};
use ultragraphdb_transactions_txn::TxnBuffers;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn literal(value: &str) -> Term {
    Literal::new_simple_literal(value).into()
}

#[test]
fn test_a_commit_is_held_once_its_round_is_synced_and_applied() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("a", "knows", "c")])?;
    driver.submit(1, prepare(txn)?)?;
    assert_eq!(driver.appended.len(), 1);
    assert_eq!(held(&applier, "a", "knows")?.len(), 1);
    assert!(driver.reply(1).is_none());
    driver.sync()?;
    let version = *driver
        .reply(1)
        .context("acknowledged")?
        .outcome()
        .as_ref()
        .context("committed")?;
    assert_eq!(applier.quads().lists().applied_ts(), version);
    assert_eq!(held(&applier, "a", "knows")?.len(), 2);
    Ok(())
}

#[test]
fn test_write_skew_is_refused() -> TestResult {
    let applier = applier()?;
    let _base = setup(
        &applier,
        vec![
            insert_literal("alice", "on-call", "true"),
            insert_literal("bob", "on-call", "true"),
        ],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    let mut second = begin(&applier)?;
    for txn in [&mut first, &mut second] {
        assert_eq!(read(txn, "alice", "on-call")?, vec![literal("true")]);
        assert_eq!(read(txn, "bob", "on-call")?, vec![literal("true")]);
    }
    write(
        &mut first,
        vec![
            delete_literal("alice", "on-call", "true"),
            insert_literal("alice", "on-call", "false"),
        ],
    )?;
    write(
        &mut second,
        vec![
            delete_literal("bob", "on-call", "true"),
            insert_literal("bob", "on-call", "false"),
        ],
    )?;
    driver.submit(1, prepare(first)?)?;
    driver.submit(2, prepare(second)?)?;
    driver.sync()?;
    driver.sync()?;
    // The first commits alone at the set-up's version plus 2 (the clock reads 0); the second, in the next round, read
    // alice's list the first rewrote.
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 0))
    );
    assert_eq!(
        driver.reply(2).context("decided")?.outcome(),
        &Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(held(&applier, "alice", "on-call")?, vec![literal("false")]);
    assert_eq!(held(&applier, "bob", "on-call")?, vec![literal("true")]);
    Ok(())
}

#[test]
fn test_inserts_of_one_new_statement_conflict_and_of_different_objects_do_not() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "z")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let txns: Vec<_> = ["b", "c", "b"]
        .iter()
        .map(|object| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert("a", "knows", object)])?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (ticket, prepared) in txns.into_iter().enumerate() {
        driver.submit(ticket as u64, prepared)?;
    }
    driver.sync()?;
    driver.sync()?;
    // 0 alone, then 1 and 2 in the next round: 1, another object of the list, commits; 2 tested the statement 0 wrote,
    // and found `b` absent, which 0 wrote a statement of: the statement is the first kind of read in the order list,
    // statement, range, absent term that a write above its read version touches.
    assert_eq!(
        driver.reply(0).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 0))
    );
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 2))
    );
    assert_eq!(
        driver.reply(2).context("decided")?.outcome(),
        &Err(CommitError::conflict(ConflictKind::Statement))
    );
    assert_eq!(held(&applier, "a", "knows")?.len(), 3);
    Ok(())
}

/// An aborted transaction's buffers come back through the step's released proposals with their capacity, as a
/// committed one's come back through its apply, and the worker's next transaction runs on them.
#[test]
fn test_an_aborted_transaction_gives_its_buffers_back() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert_literal("x", "name", "X")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let buffers = TxnBuffers::with_capacity(4_096, 64, 8_192);
    let (read_cap, record_cap) = (buffers.read_set_capacity(), buffers.record_capacity());
    let mut loser = begin_with(&applier, buffers)?;
    let _read: Vec<Term> = read(&mut loser, "x", "name")?;
    write(&mut loser, vec![insert("l", "knows", "b")])?;
    let mut winner = begin(&applier)?;
    write(
        &mut winner,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X1"),
        ],
    )?;
    driver.submit(1, prepare(winner)?)?;
    driver.submit(2, prepare(loser)?)?;
    driver.sync()?;
    driver.sync()?;
    assert_eq!(
        driver.reply(2).map(|r| r.outcome().clone()),
        Some(Err(CommitError::conflict(ConflictKind::List)))
    );
    assert_eq!(driver.released.len(), 1);
    let (ticket, back) = driver.released.pop().context("released")?;
    assert_eq!(ticket, 2);
    assert_eq!(
        (back.read_set_capacity(), back.record_capacity()),
        (read_cap, record_cap)
    );
    let mut next = begin_with(&applier, back)?;
    write(&mut next, vec![insert("n", "knows", "b")])?;
    driver.submit(3, prepare(next)?)?;
    driver.sync()?;
    // 1 at crate::utils::fixture::version(1, 0); 2 aborted and took none; 3 alone in the next round.
    assert_eq!(
        driver.reply(1).map(|r| r.outcome().clone()),
        Some(Ok(crate::utils::fixture::version(1, 0)))
    );
    assert_eq!(
        driver.reply(3).map(|r| r.outcome().clone()),
        Some(Ok(crate::utils::fixture::version(1, 2)))
    );
    assert!(held(&applier, "l", "knows")?.is_empty());
    Ok(())
}

/// Catalog version 2 of `tenant` starting an index build of `predicate`, executed at the watermark.
fn build_in(
    applier: &crate::utils::fixture::Applier,
    tenant: u64,
    predicate: &str,
) -> TestResult<ultragraphdb_transactions_txn::Prepared> {
    let version = crate::utils::fixture::term_version(tenant, 2, predicate);
    crate::utils::fixture::catalog_proposal(applier, &version)
}

/// Two catalog versions starting index builds in different tenants conflict on no key, and one build runs at a time:
/// the second waits in the step, set aside, until the first's end is admitted, in a round of its own, then is decided,
/// and every one commits (decided 2026-10-28).
#[test]
fn test_a_second_build_waits_for_the_first_builds_end() -> TestResult {
    use ultragraphdb_core_mapping::Changeset;
    use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "name", "apple")])?;
    let _base = applier
        .apply(&Changeset::new(1).with(insert_literal("a", "name", "pear")))
        .context("tenant 1 holds data")?
        .commit_ts();
    applier.sequence_builds();
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("x", "knows", "y")])?;
    driver.submit(1, prepare(first)?)?;
    driver.submit(2, build_in(&applier, 0, "name")?)?;
    driver.submit(3, build_in(&applier, 1, "name")?)?;
    driver.sync()?;
    assert_eq!(
        (driver.step.waiting(), driver.step.deferred()),
        (0, 1),
        "the second build waits while the first runs"
    );
    driver.sync()?;
    assert!(applier.rebuilding(), "the first build runs");
    applier.executor().run_until_idle();
    let end = applier
        .take_build_end()
        .context("the first build is done")?;
    driver.submit(4, Prepared::end_build(&applier, end, TxnBuffers::new())?)?;
    assert_eq!(
        driver.step.deferred(),
        0,
        "back in the queue once the end is admitted"
    );
    driver.sync()?;
    driver.sync()?;
    assert!(applier.rebuilding(), "the second build runs");
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1, 1], "the end alone in its round");
    // One transaction a round, each at the version after the last: 1, then the first build 2, its end 4, then the second
    // build 3, set aside until the end was admitted.
    let replies: Vec<(u64, Result<u64, CommitError>)> = driver
        .replies
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect();
    assert_eq!(
        replies,
        vec![
            (1, Ok(crate::utils::fixture::version(1, 0))),
            (2, Ok(crate::utils::fixture::version(1, 2))),
            (4, Ok(crate::utils::fixture::version(1, 4))),
            (3, Ok(crate::utils::fixture::version(1, 6)))
        ]
    );
    Ok(())
}

/// A drop closes its round: the transaction waiting after it is decided in the next round (7.7).
#[test]
fn test_a_round_closes_after_a_drop() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(
        &applier,
        vec![insert("a", "p", "b"), insert("w", "knows", "y")],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("x", "knows", "y")])?;
    driver.submit(1, prepare(first)?)?;
    let drop = ultragraphdb_transactions_txn::Prepared::drop(
        &applier,
        (crate::utils::fixture::TENANT, 0),
        ultragraphdb_store_apply::DropRequest::Predicate(crate::utils::fixture::ex("p").as_ref()),
        0,
        0,
        0,
        TxnBuffers::new(),
    )
    .context("the drop prepares")?;
    driver.submit(2, drop)?;
    let mut after = begin(&applier)?;
    write(&mut after, vec![insert("z", "knows", "y")])?;
    driver.submit(3, prepare(after)?)?;
    driver.sync()?;
    driver.sync()?;
    driver.sync()?;
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1]);
    let reply = |t: u64| {
        driver
            .reply(t)
            .context("decided")
            .map(|r| r.outcome().clone())
    };
    assert_eq!(reply(1)?, Ok(crate::utils::fixture::version(1, 0)));
    assert_eq!(
        reply(2)?,
        Ok(crate::utils::fixture::version(1, 2)),
        "the drop takes its round's last version"
    );
    // The drop removes `p` alone: the catalog is read through its version, not a key range, so a transaction writing
    // another predicate commits in the round after the drop's, at the next version.
    assert_eq!(reply(3)?, Ok(crate::utils::fixture::version(1, 4)));
    assert!(held(&applier, "a", "p")?.is_empty());
    assert_eq!(held(&applier, "z", "knows")?.len(), 1);
    Ok(())
}

/// C4.17: a Drop of a predicate or a graph that finds nothing to remove closes its round as a drop removing something does
/// (`PROTOCOL.md` §6.5, "the last transaction of its round"; §12.2, "the last record of its round"): the transaction
/// waiting after it is decided in the next round, and each of the three takes a round of its own.
#[test]
fn test_a_round_closes_after_a_drop_that_finds_nothing() -> TestResult {
    use ultragraphdb_store_apply::DropRequest;
    let predicate = crate::utils::fixture::ex("no-such-predicate");
    let graph = crate::utils::fixture::ex("no-such-graph");
    for drop in [
        DropRequest::Predicate(predicate.as_ref()),
        DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
    ] {
        let applier = applier()?;
        let _version: u64 = setup(&applier, vec![insert("w", "knows", "y")])?;
        let mut driver = Driver::new(&applier, unbounded());
        let mut first = begin(&applier)?;
        write(&mut first, vec![insert("x", "knows", "y")])?;
        driver.submit(1, prepare(first)?)?;
        let prepared = ultragraphdb_transactions_txn::Prepared::drop(
            &applier,
            (crate::utils::fixture::TENANT, 0),
            drop,
            0,
            0,
            0,
            TxnBuffers::new(),
        )
        .context("the drop prepares")?;
        driver.submit(2, prepared)?;
        let mut after = begin(&applier)?;
        write(&mut after, vec![insert("z", "knows", "y")])?;
        driver.submit(3, prepare(after)?)?;
        let tickets = [1, 2, 3];
        for _ in tickets {
            driver.sync()?;
        }
        let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
        assert_eq!(
            counts,
            vec![1; tickets.len()],
            "{drop:?}: one decision a round, the drop's round closed after it"
        );
        for ticket in tickets {
            let outcome = driver.reply(ticket).context("decided")?.outcome().clone();
            assert!(outcome.is_ok(), "{drop:?}, ticket {ticket}: {outcome:?}");
        }
        assert_eq!(held(&applier, "z", "knows")?.len(), 1);
    }
    Ok(())
}

/// C4.20 at the commit step: with version 1 in effect, a transaction executed against it and a Drop of a predicate holding
/// data, whose command names `catalog` 0, are decided in one round, and every record of the round names the version in
/// effect (`PROTOCOL.md` §12.1, "each tenant's records name the one catalog version in effect at it"; §12.2), so the round
/// reads back. The Drop is checked against no version (§6.5) and commits.
#[test]
fn test_a_drop_of_a_predicate_names_the_version_in_effect_in_its_round() -> TestResult {
    use ultragraphdb_cluster_sequencer::CatalogVersion;
    use ultragraphdb_core_ids::Tenant;
    use ultragraphdb_protocol_wire::{RoundHeader, RoundView};
    use ultragraphdb_store_apply::DropRequest;
    use ultragraphdb_transactions_commit::CommitStep;
    let in_effect = 1;
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "p", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.step =
        CommitStep::new(unbounded(), applier.quads().lists().applied_ts()).with_catalog(&[
            CatalogVersion::new(Tenant::new(crate::utils::fixture::TENANT), in_effect),
        ]);
    // The first transaction's round is in flight while the next two arrive, so those two share the next round.
    let mut first = begin(&applier)?.with_catalog(in_effect);
    write(&mut first, vec![insert("x", "knows", "y")])?;
    driver.submit(1, prepare(first)?)?;
    let mut txn = begin(&applier)?.with_catalog(in_effect);
    write(&mut txn, vec![insert("z", "knows", "y")])?;
    driver.submit(2, prepare(txn)?)?;
    let drop = ultragraphdb_transactions_txn::Prepared::drop(
        &applier,
        (crate::utils::fixture::TENANT, 0),
        DropRequest::Predicate(crate::utils::fixture::ex("p").as_ref()),
        0,
        0,
        0,
        TxnBuffers::new(),
    )
    .context("the drop prepares")?;
    driver.submit(3, drop)?;
    driver.sync()?;
    driver.sync()?;
    for ticket in [1, 2, 3] {
        let outcome = driver.reply(ticket).context("decided")?.outcome().clone();
        assert!(outcome.is_ok(), "ticket {ticket}: {outcome:?}");
    }
    let round = *driver.appended.get(1).context("the second round")?;
    let body = driver.bodies.get(1).context("its body")?;
    assert_eq!(round.transactions(), 2, "the transaction and the Drop");
    let header = RoundHeader::new(
        2,
        0,
        0,
        round.first_version(),
        round.transactions(),
        std::num::NonZeroU32::MIN,
    );
    let view = RoundView::from_body(header, body)
        .map_err(|e| ultragraphdb_tool_bdd::TestError::new(e.to_string()))?;
    let catalogs: Vec<u64> = view.records().iter().map(|r| r.catalog()).collect();
    assert_eq!(catalogs, vec![in_effect; view.records().len()]);
    assert!(held(&applier, "a", "p")?.is_empty(), "the Drop removed `p`");
    Ok(())
}

/// A Drop of a tenant's data is decided against the catalog version in effect at its round, 0 here: one naming version 1 is
/// refused with a conflict of kind Catalog, code 2, kind 5 (`PROTOCOL.md` §6.5), and removes nothing; one naming 0 commits.
#[test]
fn test_a_drop_of_a_tenants_data_is_decided_against_the_version_in_effect() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "p", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    for (ticket, catalog) in [(1, 1), (2, 0)] {
        let drop = ultragraphdb_transactions_txn::Prepared::drop(
            &applier,
            (crate::utils::fixture::TENANT, catalog),
            ultragraphdb_store_apply::DropRequest::Data,
            0,
            0,
            0,
            TxnBuffers::new(),
        )
        .context("the drop prepares")?;
        driver.submit(ticket, drop)?;
        driver.sync()?;
        // Held after the refused one, gone after the other.
        assert_eq!(
            held(&applier, "a", "p")?.is_empty(),
            ticket == 2,
            "after Drop {ticket}"
        );
    }
    let reply = |t: u64| {
        driver
            .reply(t)
            .context("decided")
            .map(|r| r.outcome().clone())
    };
    assert_eq!(reply(1)?, Err(CommitError::conflict(ConflictKind::Catalog)));
    assert!(reply(2)?.is_ok(), "{:?}", reply(2));
    Ok(())
}

/// C4.15 for a Drop that finds nothing to remove: with version 4 in effect, a Drop of a predicate the tenant does not hold
/// and one of a graph it does not hold name `catalog` 0, are checked against no version and commit (`PROTOCOL.md` §6.5),
/// removing nothing. A transaction that wrote and read nothing, whose record is the same but for its session, is still
/// checked: executed against version 0, it conflicts naming the catalog (§11.13, *Transactions against a version*).
#[test]
fn test_a_drop_of_a_predicate_or_a_graph_finding_nothing_is_not_checked() -> TestResult {
    use ultragraphdb_cluster_sequencer::CatalogVersion;
    use ultragraphdb_core_ids::Tenant;
    use ultragraphdb_store_apply::DropRequest;
    use ultragraphdb_transactions_commit::CommitStep;
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "p", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.step =
        CommitStep::new(unbounded(), applier.quads().lists().applied_ts()).with_catalog(&[
            CatalogVersion::new(Tenant::new(crate::utils::fixture::TENANT), 4),
        ]);
    let predicate = crate::utils::fixture::ex("no-such-predicate");
    let graph = crate::utils::fixture::ex("no-such-graph");
    for (ticket, drop) in [
        (1, DropRequest::Predicate(predicate.as_ref())),
        (
            2,
            DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
        ),
    ] {
        let prepared = ultragraphdb_transactions_txn::Prepared::drop(
            &applier,
            (crate::utils::fixture::TENANT, 0),
            drop,
            0,
            0,
            0,
            TxnBuffers::new(),
        )
        .context("the drop prepares")?;
        driver.submit(ticket, prepared)?;
        driver.sync()?;
        let outcome = driver.reply(ticket).context("decided")?.outcome().clone();
        assert!(outcome.is_ok(), "{drop:?}: {outcome:?}");
    }
    assert_eq!(held(&applier, "a", "p")?.len(), 1, "nothing removed");
    driver.submit(3, prepare(begin(&applier)?)?)?;
    driver.sync()?;
    assert_eq!(
        driver
            .reply(3)
            .context("decided")
            .map(|r| r.outcome().clone())?,
        Err(CommitError::conflict(ConflictKind::Catalog))
    );
    Ok(())
}

/// The driver's closes and clock reach the step over the real store: a close opens the next epoch, whose first commit
/// takes sequence 0, and a tick with nothing held changes nothing.
#[test]
fn test_a_close_through_the_driver_opens_the_next_epoch() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.close(1)?;
    driver.close_with(2, Vec::new(), vec![7])?;
    driver.tick(1_000)?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "b")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.sync()?;
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(3, 0))
    );
    assert!(driver.close_requests.is_empty());
    Ok(())
}
