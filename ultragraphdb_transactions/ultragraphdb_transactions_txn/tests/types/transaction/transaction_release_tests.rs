/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A transaction ending without a commit releases what it holds (transactions, "An ended transaction releases what it
//! holds"): the dictionary entries it created are let go at its abort, at a refusal ending it, at its lifetime limit, at
//! a refusal of its commit and when it is dropped. A writer of `<new>` shows whether the transaction still holds the ID
//! it created for `<new>`: it takes that ID while the transaction holds it, and is given another once it is released.

use std::time::Duration;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_txn::{Mode, TxnError, TxnErrorEnum, TxnLimits};

use crate::utils::fixture::{
    Applier, SECOND, Txn, applier, begin, begin_with, changeset, ex, held, id_in, insert,
    insert_literal, prepared, store,
};

/// The ID a writer of `<new>` is given now, its proposal released at once: the ID another write holds, or a new one.
fn id_now(applier: &Applier, request: u64) -> TestResult<SystemId> {
    let mut writer = begin(applier, Mode::ReadWrite)?;
    writer
        .write(&changeset(vec![insert("new", "p", "q")]), 0)
        .context("the writer writes `<new>`")?;
    let proposal = prepared(writer.commit(1, request, &[], 0))?;
    let id = id_in(&proposal, "new")?;
    let _buffers = proposal.release(applier);
    Ok(id)
}

/// `txn` creating `<new>`, and the ID it created, which it holds: two writers of `<new>` take it in turn.
fn creating(applier: &Applier, txn: &mut Txn<'_>) -> TestResult<SystemId> {
    txn.write(&changeset(vec![insert("new", "p", "q")]), 0)
        .context("the write creates `<new>`")?;
    let created = id_now(applier, 1)?;
    assert_eq!(
        id_now(applier, 2)?,
        created,
        "the open transaction holds `<new>`"
    );
    Ok(created)
}

#[test]
fn test_an_aborted_transaction_releases_what_it_created_and_writes_nothing() -> TestResult {
    let applier = applier()?;
    let watermark = applier.quads().lists().applied_ts();
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let created = creating(&applier, &mut txn)?;
    let _buffers = txn.abort();
    assert_ne!(id_now(&applier, 3)?, created, "the abort released `<new>`");
    assert!(held(&applier, "new", "p")?.is_empty(), "nothing is written");
    assert_eq!(
        applier
            .quads()
            .id_of(0, ex("new").as_str())
            .context("the dictionary reads")?,
        None,
        "the store holds no entry for `<new>`"
    );
    assert_eq!(applier.quads().lists().applied_ts(), watermark);
    Ok(())
}

#[test]
fn test_a_transaction_ended_by_a_refused_changeset_releases_what_it_created() -> TestResult {
    let applier = applier()?;
    // `age` declared an integer: a string object is refused by the catalog.
    let declared =
        PredicateDeclaration::new(ex("age")).with_datatype(oxrdf::vocab::xsd::INTEGER.into_owned());
    store(
        &applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let created = creating(&applier, &mut txn)?;
    let refused = txn.write(&changeset(vec![insert_literal("a", "age", "old")]), 0);
    assert!(
        matches!(&refused, Err(TxnError(TxnErrorEnum::Changeset(_)))),
        "the catalog refuses it: {refused:?}"
    );
    assert!(txn.is_ended());
    assert_ne!(
        id_now(&applier, 3)?,
        created,
        "the refusal ending it released `<new>`, the transaction still in scope"
    );
    Ok(())
}

#[test]
fn test_a_transaction_past_its_limit_releases_at_its_next_request() -> TestResult {
    let applier = applier()?;
    let limits = TxnLimits::default().with_read_write(Duration::from_secs(1));
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    let created = creating(&applier, &mut txn)?;
    let late = txn.write(&changeset(vec![insert("new", "p", "r")]), 2 * SECOND);
    assert_eq!(late, Err(TxnError::past_limit(1_000)));
    assert!(txn.is_ended());
    assert_ne!(
        id_now(&applier, 3)?,
        created,
        "the limit ending it released `<new>`"
    );
    Ok(())
}

#[test]
fn test_a_commit_past_the_limit_releases() -> TestResult {
    let applier = applier()?;
    let limits = TxnLimits::default().with_read_write(Duration::from_secs(1));
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    let created = creating(&applier, &mut txn)?;
    let refused = txn.commit(1, 1, &[], 2 * SECOND).err();
    assert_eq!(refused, Some(TxnError::past_limit(1_000)));
    assert_ne!(
        id_now(&applier, 3)?,
        created,
        "the refused commit released `<new>`"
    );
    Ok(())
}

#[test]
fn test_a_commit_refused_past_the_write_set_limit_releases() -> TestResult {
    let applier = applier()?;
    let limits = TxnLimits::default().with_max_write_set(1);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    let created = creating(&applier, &mut txn)?;
    let refused = txn.commit(1, 1, &[], 0).err().context("the commit fails")?;
    assert!(
        matches!(refused.0, TxnErrorEnum::WriteSetTooLarge { .. }),
        "{refused:?}"
    );
    assert_ne!(
        id_now(&applier, 3)?,
        created,
        "the refused commit released `<new>`"
    );
    assert!(held(&applier, "new", "p")?.is_empty());
    Ok(())
}

#[test]
fn test_a_commit_whose_finish_the_store_fails_releases_once() -> TestResult {
    let applier = applier()?;
    // A first commit, so the commit below writes nothing of the namespace's initial catalog.
    store(&applier, vec![insert("a", "p", "b")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let created = creating(&applier, &mut txn)?;
    // A second transaction takes `<new>` from the first: releasing the first twice would take its hold away too.
    let mut taker = begin(&applier, Mode::ReadWrite)?;
    taker
        .write(&changeset(vec![insert("new", "p", "r")]), 0)
        .context("the taker writes `<new>`")?;
    applier
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextRead);
    let refused = txn.commit(1, 1, &[], 0).err().context("the commit fails")?;
    assert!(
        matches!(refused.0, TxnErrorEnum::Changeset(_)),
        "the store's failure: {refused:?}"
    );
    assert_eq!(
        id_now(&applier, 3)?,
        created,
        "released once, `<new>` is still the taker's"
    );
    let _buffers = taker.abort();
    assert_ne!(
        id_now(&applier, 4)?,
        created,
        "the failed commit released `<new>`"
    );
    Ok(())
}

#[test]
fn test_a_dropped_transaction_releases() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let created = creating(&applier, &mut txn)?;
    drop(txn);
    assert_ne!(
        id_now(&applier, 3)?,
        created,
        "dropping it released `<new>`"
    );
    assert!(held(&applier, "new", "p")?.is_empty());
    Ok(())
}

#[test]
fn test_a_read_write_commit_hands_its_holds_to_the_proposal() -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let created = creating(&applier, &mut txn)?;
    let proposal = prepared(txn.commit(1, 1, &[], 0))?;
    assert_eq!(id_in(&proposal, "new")?, created);
    assert_eq!(
        id_now(&applier, 3)?,
        created,
        "the proposal holds `<new>` until the commit step applies or releases it"
    );
    let _buffers = proposal.release(&applier);
    assert_ne!(
        id_now(&applier, 4)?,
        created,
        "the released proposal let go of `<new>`"
    );
    Ok(())
}
