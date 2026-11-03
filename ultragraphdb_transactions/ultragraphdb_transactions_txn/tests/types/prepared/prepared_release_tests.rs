/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A proposal the commit step will not apply is released (transactions, "An ended transaction releases what it
//! holds"): an ID no other write holds is never used again, and one another write took stays that write's.

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_txn::Mode;

use crate::utils::fixture::{applier, begin, changeset, id_in, insert_literal, prepared};

#[test]
fn test_a_released_entry_is_never_used_again_and_one_taken_stays_the_takers() -> TestResult {
    let applier = applier()?;
    let mut creator = begin(&applier, Mode::ReadWrite)?;
    creator
        .write(&changeset(vec![insert_literal("new", "p", "x")]), 0)
        .context("the creator creates `<new>`")?;
    // A transaction writing `<new>` while the creator is open takes its ID.
    let mut taker = begin(&applier, Mode::ReadWrite)?;
    taker
        .write(&changeset(vec![insert_literal("new", "p", "y")]), 0)
        .context("the taker writes `<new>`")?;
    let taker = prepared(taker.commit(1, 1, &[], 0))?;
    let taken = id_in(&taker, "new")?;
    let _creator_buffers = creator.abort();
    // The taker still holds the ID: a third writer takes it too.
    let mut third = begin(&applier, Mode::ReadWrite)?;
    third
        .write(&changeset(vec![insert_literal("new", "p", "z")]), 0)
        .context("the third writes `<new>`")?;
    let third = prepared(third.commit(1, 2, &[], 0))?;
    assert_eq!(
        id_in(&third, "new")?,
        taken,
        "the ID the taker holds is kept"
    );
    let _taker_buffers = taker.release(&applier);
    let _third_buffers = third.release(&applier);
    // No write holds it now: the next writer of `<new>` creates another.
    let mut later = begin(&applier, Mode::ReadWrite)?;
    later
        .write(&changeset(vec![insert_literal("new", "p", "w")]), 0)
        .context("the later writer writes `<new>`")?;
    let later = prepared(later.commit(1, 3, &[], 0))?;
    assert_ne!(
        id_in(&later, "new")?,
        taken,
        "a released ID is never used again"
    );
    let _later_buffers = later.release(&applier);
    Ok(())
}
