/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "A request declares its type".

use cucumber::{then, when};

use ultragraphdb_client_core::ClientErrorEnum;
use ultragraphdb_protocol_wire::{
    Command, CommitOperation, ConditionalWrite, Declared, DeclaredPredicate, DirectWrite,
    ErrorCode, RequestTerm, TermTable, WriteChange,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

use crate::in_doubt_steps::{count, insert};
use crate::utils::harness::{Harness, committed, server};
use crate::{Steps, session};

#[when("a client sends a DirectWrite inserting `<a> <knows> <b>` through its session")]
fn one_insert(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let p = h.core.direct_write(insert(), session, h.now)?;
    w.answers.push(committed(h.settle(p)?)?);
    w.harness = Some(h);
    Ok(())
}

#[then(
    "the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply"
)]
fn committed_in_one(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    let answer = w.answers.first().context("the reply")?;
    assert_eq!(answer.version(), h.server.version());
    assert!(answer.token().is_some());
    assert_eq!(h.server.applied(), 1);
    assert_eq!(h.server.open_transactions(), 0);
    assert_eq!(count(h, Command::Begin), 0, "no Begin");
    Ok(())
}

#[when("a ConditionalWrite holds a compare-and-set whose expected value does not hold")]
fn condition_fails(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let mut terms = TermTable::new();
    let subject = terms.iri("http://ex.org/a");
    let predicate = terms.iri("http://ex.org/state");
    let expected = terms.iri("http://ex.org/open");
    let new = terms.iri("http://ex.org/closed");
    h.server
        .fail_next_condition(RequestTerm::Iri("http://ex.org/held".into()));
    let write = ConditionalWrite::new(
        0,
        terms,
        Vec::new(),
        vec![CommitOperation::CompareAndSet {
            graph: None,
            subject,
            predicate,
            expected,
            new,
        }],
    );
    let p = h.core.conditional_write(write, session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("refused")?);
    w.harness = Some(h);
    Ok(())
}

#[then(
    "it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written"
)]
fn an_answer(w: &mut Steps) -> TestResult {
    let answer = w.errors.first().context("the refusal")?;
    let ClientErrorEnum::Refused { code, value, .. } = &answer.0 else {
        return Err(TestError::new(format!("{answer:?}")));
    };
    assert_eq!(*code, ErrorCode::OperationFailed);
    assert_eq!(value, &Some(RequestTerm::Iri("http://ex.org/held".into())));
    assert!(!answer.retryable());
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(count(h, Command::ConditionalWrite), 1);
    assert_eq!(h.server.applied(), 0);
    Ok(())
}

#[when("a client loses the reply to request 7 of its session, a DirectWrite, and sends it again")]
fn lost_write(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let session = h.core.resume_session(session.number(), 7)?;
    h.server.drop_after_next_commit();
    let p = h.core.direct_write(insert(), session, h.now)?;
    w.answers.push(committed(h.settle(p)?)?);
    w.harness = Some(h);
    Ok(())
}

#[then("it receives the same reply and commit version, and the writes are held once")]
fn held_once(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    let answer = w.answers.first().context("the reply")?;
    assert_eq!(answer.request(), 7);
    assert_eq!(answer.version(), h.server.version());
    assert_eq!(h.server.applied(), 1);
    assert_eq!(count(h, Command::DirectWrite), 2);
    Ok(())
}

#[when("a DirectWrite holds a declaration of a predicate")]
fn declaration(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let write = DirectWrite::new(
        0,
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new("http://ex.org/p"),
        ))],
    );
    let p = h.core.direct_write(write, session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("refused")?);
    w.harness = Some(h);
    Ok(())
}

#[then("it is refused with a protocol error, and the catalog is unchanged")]
fn refused_as_protocol(w: &mut Steps) -> TestResult {
    let answer = w.errors.first().context("the refusal")?;
    assert!(
        matches!(
            answer.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::Protocol,
                ..
            }
        ),
        "{answer:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(h.server.applied(), 0);
    Ok(())
}
