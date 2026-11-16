/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "A Describe's answer continues across frames".

use cucumber::{then, when};

use ultragraphdb_client_core::ClientErrorEnum;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    CATALOG_BYTES_MAX, Command, DeclaredPredicate, Describe, DescribeEntry, EntryKind, Header,
    HeaderView, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

use crate::Steps;
use crate::utils::harness::{CLUSTER, Harness, described, server};

/// The line a source repeats: 19 bytes of ASCII.
const LINE: &str = "namespace gen5 { }\n";

/// An answer of tenant 0 read at version 42, laid out by hand: a label, a declared predicate, and an installed source of
/// `lines` lines, which decides how many frames it takes.
fn answer(lines: usize) -> Describe {
    Describe::new(0, 42)
        .with_labels(vec!["Person".to_string()])
        .with_predicates(vec![DeclaredPredicate::new("http://ex.org/name")])
        .with_entries(vec![DescribeEntry::new(
            EntryKind::Source,
            "gen5",
            LINE.repeat(lines).into_bytes(),
        )])
}

#[when("a tenant's Describe answer takes two frames within the client's `max_frame`")]
fn two_frames(w: &mut Steps) -> TestResult {
    // 78,948 lines are 1,500,012 bytes: more than one frame of 1 MiB holds, less than two.
    let expected = answer(78_948);
    let frames = expected.frames(Header::new(CLUSTER, Command::Describe), MAX_FRAME_SIZE)?;
    if frames.len() != 2 {
        return Err(TestError::new(format!("{} frames", frames.len())));
    }
    let mut h = Harness::ready(server().with_describe(expected.clone()))?;
    let p = h.core.describe(0, h.now)?;
    w.describes = vec![expected, described(h.settle(p)?)?];
    w.harness = Some(h);
    Ok(())
}

#[then("the client reads the declarations one frame would hold, within the Welcome's credit")]
fn read_whole(w: &mut Steps) -> TestResult {
    let [expected, read] = w.describes.as_slice() else {
        return Err(TestError::new("an answer expected and one read"));
    };
    assert_eq!(read, expected);
    let h = w.harness.as_ref().context("a harness")?;
    let credit = Command::Credit.number();
    assert!(
        h.server
            .received()
            .iter()
            .all(|frame| HeaderView::new(frame).is_ok_and(|header| header.command() != credit)),
        "no Credit"
    );
    Ok(())
}

#[when(
    "a server sends a Describe answer whose sections hold more than `catalog_bytes_max` bytes together"
)]
fn past_the_bound(w: &mut Steps) -> TestResult {
    // Two MiB past the bound in the source alone: frames remain after the one that passes it.
    let lines = (CATALOG_BYTES_MAX + (2 << 20)) / LINE.len();
    let mut h = Harness::ready(server().with_describe(answer(lines)))?;
    let p = h.core.describe(0, h.now)?;
    w.errors.push(h.settle(p)?.err().context("refused")?);
    w.harness = Some(h);
    Ok(())
}

#[then("the client refuses it as `FieldValue` naming `describe`, and closes the connection")]
fn refused(w: &mut Steps) -> TestResult {
    let error = w.errors.first().context("an answer")?;
    let ClientErrorEnum::ConnectionLost { cause: Some(cause) } = &error.0 else {
        return Err(TestError::new(format!("{error:?}")));
    };
    let bound = CATALOG_BYTES_MAX as u64; // lossless: a `usize` is at most 64 bits
    assert!(
        matches!(
            &cause.0,
            ClientErrorEnum::Protocol { error } if matches!(
                error.0,
                ProtocolErrorEnum::FieldValue { command: 12, field: "describe", value } if value > bound
            )
        ),
        "{cause:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert!(!h.core.is_connected());
    Ok(())
}
