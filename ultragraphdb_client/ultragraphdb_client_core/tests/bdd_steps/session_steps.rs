/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "Commits answer a session token".

use cucumber::{then, when};

use ultragraphdb_client_core::Reply;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{
    BodyView, Command, CommitOperation, CommitView, ConditionalWrite, ConditionalWriteView,
    DirectWriteView, DropCommand, FrameView, HeaderView, MAX_FRAME_SIZE, TermTable,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

use crate::in_doubt_steps::insert;
use crate::utils::harness::{CLUSTER, Harness, committed, dropped, server};
use crate::{Steps, begin, session};

/// A ConditionalWrite of one counter add.
pub(crate) fn counter() -> ConditionalWrite {
    let mut terms = TermTable::new();
    let subject = terms.iri("http://ex.org/a");
    let predicate = terms.iri("http://ex.org/count");
    ConditionalWrite::new(
        0,
        terms,
        Vec::new(),
        vec![CommitOperation::CounterAdd {
            graph: None,
            subject,
            predicate,
            delta: 1,
        }],
    )
}

#[when("a transaction commits in partition 1 at position 40 with a version of epoch 12")]
fn commits_at_epoch_twelve(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.set_epoch(12);
    h.server.set_position(40);
    let tx = begin(&mut h, false)?;
    let p = h.core.commit(tx, session, h.now)?;
    w.answers.push(committed(h.settle(p)?)?);
    Ok(())
}

#[then("its reply carries the token (partition 1, position 40, epoch 12)")]
fn carries_the_token(w: &mut Steps) -> TestResult {
    let token = w
        .answers
        .first()
        .and_then(|a| a.token())
        .context("a token")?;
    assert_eq!(
        (token.partition(), token.position(), token.epoch()),
        (1, 40, Epoch::new(12)?)
    );
    Ok(())
}

#[when("a transaction and a Drop each commit through no session, `session` all zero")]
fn through_no_session(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h, false)?;
    let p = h.core.commit(tx, None, h.now)?;
    w.answers.push(committed(h.settle(p)?)?);
    w.numbers.push(h.server.version());
    let p2 = h
        .core
        .drop_command(DropCommand::tenant_data(0), None, h.now)?;
    w.answers.push(dropped(h.settle(p2)?)?);
    w.numbers.push(h.server.version());
    w.harness = Some(h);
    Ok(())
}

#[then(
    "each reply's `commit_version` is its version and its token is 24 zero bytes, and a receiver accepts it"
)]
fn zero_tokens(w: &mut Steps) -> TestResult {
    let versions: Vec<u64> = w.answers.iter().map(|a| a.version()).collect();
    assert_eq!(versions, w.numbers);
    assert!(w.answers.iter().all(|a| a.token().is_none()));
    let h = w.harness.as_ref().context("a harness")?;
    assert!(h.core.is_ready(), "the client accepted both");
    Ok(())
}

#[when(
    "a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored reply's is of epoch 9, then sends a ConditionalWrite through the session"
)]
fn highest_epoch_kept(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.set_epoch(12);
    let p = h.core.direct_write(insert(), session, h.now)?;
    let _first = committed(h.settle(p)?)?;
    h.server.set_epoch(9);
    h.server.drop_after_next_commit();
    let p2 = h.core.direct_write(insert(), session, h.now)?;
    let resolved = committed(h.settle(p2)?)?;
    assert_eq!(
        resolved.token().map(|t| t.epoch()),
        Some(Epoch::new(9)?),
        "the stored reply's own"
    );
    let p3 = h.core.conditional_write(counter(), session, h.now)?;
    assert!(matches!(h.settle(p3)?, Ok(Reply::Committed(_))));
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    let write = ConditionalWriteView::from_frame(&view)?;
    w.numbers
        .push(write.token().context("a token")?.epoch().get());
    w.numbers.push(write.principal());
    Ok(())
}

#[then("the ConditionalWrite carries the token of epoch 12")]
fn carries_epoch_twelve(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.numbers,
        vec![12, 0],
        "the token of epoch 12, the principal 0"
    );
    Ok(())
}

#[when(
    "a client's session holds a token of epoch 12, and a commit through no session answers the zero token"
)]
fn zero_after_twelve(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.set_epoch(12);
    let tx = begin(&mut h, false)?;
    let p = h.core.commit(tx, session, h.now)?;
    let _held = committed(h.settle(p)?)?;
    let tx2 = begin(&mut h, false)?;
    let p2 = h.core.commit(tx2, None, h.now)?;
    let reply = committed(h.settle(p2)?)?;
    assert!(reply.token().is_none());
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then("the session still sends the token of epoch 12")]
fn still_sends_twelve(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let h = w.harness.as_mut().context("a harness")?;
    let p = h.core.direct_write(insert(), session, h.now)?;
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    let sent = h
        .server
        .received()
        .iter()
        .rev()
        .find(|f| HeaderView::new(f).is_ok_and(|v| v.command() == Command::DirectWrite.number()))
        .context("the write")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    let write = DirectWriteView::from_frame(&view)?;
    assert_eq!(write.token().context("a token")?.epoch(), Epoch::new(12)?);
    let replies = h
        .server
        .received()
        .iter()
        .filter_map(|f| FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE).ok())
        .filter(|v| CommitView::from_frame(v).is_ok())
        .count();
    assert_eq!(replies, 2, "the two commits");
    Ok(())
}
