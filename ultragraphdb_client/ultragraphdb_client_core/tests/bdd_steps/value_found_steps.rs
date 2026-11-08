/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "An OperationFailed Error carries the value found within the floor of `max_frame`".

use cucumber::{then, when};

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_protocol_wire::{
    CommitOperation, ConditionalWrite, ErrorCode, MAX_FRAME_FLOOR, RequestTerm, TermTable,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

use crate::utils::harness::{Harness, server};
use crate::{Steps, session};

/// A compare-and-set of `<a> <state>` from `<open>` to `<closed>`.
fn compare_and_set() -> ConditionalWrite {
    let mut terms = TermTable::new();
    let subject = terms.iri("http://ex.org/a");
    let predicate = terms.iri("http://ex.org/state");
    let expected = terms.iri("http://ex.org/open");
    let new = terms.iri("http://ex.org/closed");
    ConditionalWrite::new(
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
    )
}

/// A plain literal of `lexical`.
fn literal(lexical: String) -> RequestTerm {
    RequestTerm::Literal {
        lexical,
        datatype: None,
    }
}

/// The lexical form of the literal `error` holds as its value found.
fn lexical(error: &ClientError) -> TestResult<&str> {
    if let ClientErrorEnum::Refused {
        code: ErrorCode::OperationFailed,
        value: Some(RequestTerm::Literal { lexical, .. }),
        ..
    } = &error.0
    {
        return Ok(lexical);
    }
    Err(TestError::new(format!(
        "a refusal holding a literal: {error:?}"
    )))
}

#[when(
    "a ConditionalWrite's compare-and-set fails on a literal of 524,288 bytes, and another on a literal as long as the first refusal carried"
)]
fn two_failures(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server
        .fail_next_condition(literal("v".repeat(MAX_FRAME_FLOOR)));
    let first_write = h.core.conditional_write(compare_and_set(), s, h.now)?;
    let first = h.settle(first_write)?.err().context("the first refusal")?;
    let carried = lexical(&first)?.len();
    w.errors.push(first);
    h.server.fail_next_condition(literal("v".repeat(carried)));
    let second_write = h.core.conditional_write(compare_and_set(), s, h.now)?;
    w.errors.push(
        h.settle(second_write)?
            .err()
            .context("the second refusal")?,
    );
    w.harness = Some(h);
    Ok(())
}

#[then(
    "the first refusal holds the literal's start, cut, and the client says it was cut; the second holds the literal whole, and the client says nothing was cut"
)]
fn cut_then_whole(w: &mut Steps) -> TestResult {
    let [first, second] = w.errors.as_slice() else {
        return Err(TestError::new(format!("two refusals: {:?}", w.errors)));
    };
    let cut = lexical(first)?;
    assert!(cut.len() < MAX_FRAME_FLOOR, "the first value is cut");
    assert!(
        cut.bytes().all(|b| b == b'v'),
        "the cut keeps the literal's start"
    );
    assert!(first.value_cut());
    assert_eq!(
        lexical(second)?,
        cut,
        "the second literal, as long as the cut, arrives whole"
    );
    assert!(!second.value_cut());
    Ok(())
}
