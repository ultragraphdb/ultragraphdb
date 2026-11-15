/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Defaults answer across continuation frames (`PROTOCOL.md` §13.28), read by `DefaultsFrames` from frames laid out
//! by hand: a frame's own rules first, then the first frame's command header, then the order running on.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{CATALOG_BYTES_MAX, Defaults, DefaultsFrames};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::defaults::defaults_tests::{header, owned};
use crate::utils::golden_bodies::frame_of;
use crate::utils::system_frames::{count, defaults_frame, field, iris, role};

/// `frames` taken in order; the refusal of the first one refused.
fn taken(frames: &[Vec<u8>]) -> TestResult<Result<DefaultsFrames, ProtocolErrorEnum>> {
    let mut taken = DefaultsFrames::new();
    for bytes in frames {
        if let Err(refusal) = taken.push(&frame_of(bytes)?) {
            return Ok(Err(refusal.0));
        }
    }
    Ok(Ok(taken))
}

/// A reply frame of tenant 3 at position 7 holding `texts`.
fn part(texts: &[&str], last: bool) -> TestResult<Vec<u8>> {
    defaults_frame(3, 7, &iris(texts), last)
}

/// Nothing is read before the first frame.
#[test]
fn test_nothing_is_read_before_the_first_frame() -> TestResult {
    for taken in [DefaultsFrames::new(), DefaultsFrames::default()] {
        assert_eq!((taken.tenant(), taken.position()), (None, None));
        assert!(taken.predicates().is_empty());
        assert!(!taken.is_last());
        assert!(!taken.is_past_bound());
    }
    Ok(())
}

/// C1.73: an answer in one frame, flagged `LAST`; the golden reply.
#[test]
fn test_an_answer_in_one_frame_reads() -> TestResult {
    let answer = taken(&[role("defaults_reply")?])?
        .ok()
        .context("the answer")?;
    assert!(answer.is_last());
    assert_eq!((answer.tenant(), answer.position()), (Some(3), Some(1_250)));
    assert_eq!(
        answer.predicates(),
        owned(&[
            "http://ex.org/Zeta",
            "http://ex.org/age",
            "http://ex.org/name"
        ])
        .as_slice()
    );
    Ok(())
}

/// C1.74: an answer across two frames and across three reads as the predicates in order, whole once the frame
/// flagged `LAST` is taken.
#[test]
fn test_an_answer_across_frames_reads_in_order() -> TestResult {
    let two = taken(&[part(&["urn:a", "urn:b"], false)?, part(&["urn:c"], true)?])?
        .ok()
        .context("two frames")?;
    assert!(two.is_last());
    assert_eq!(two.predicates(), owned(&["urn:a", "urn:b", "urn:c"]));
    assert_eq!((two.tenant(), two.position()), (Some(3), Some(7)));

    let mut three = DefaultsFrames::new();
    for (texts, last) in [(["urn:a"], false), (["urn:b"], false), (["urn:c"], true)] {
        assert!(!three.is_last());
        three.push(&frame_of(&part(&texts, last)?)?)?;
    }
    assert!(three.is_last());
    assert_eq!(three.predicates(), owned(&["urn:a", "urn:b", "urn:c"]));
    Ok(())
}

/// C1.74 through the builder: what `Defaults::frames` cuts at one predicate a frame reads back as the answer.
#[test]
fn test_what_the_builder_cuts_reads_back() -> TestResult {
    let texts = owned(&["urn:a", "urn:b", "urn:c", "urn:d"]);
    // 256 bytes of header, 16 of section table, a record of 24 and a text of 5 bytes: one predicate a frame.
    let frames = Defaults::reply(3, 7, texts.clone()).frames(header(), 256 + 16 + 24 + 5)?;
    assert_eq!(frames.len(), 4);
    let answer = taken(&frames)?.ok().context("the answer")?;
    assert!(answer.is_last());
    assert_eq!(answer.predicates(), texts.as_slice());
    Ok(())
}

/// C1.75: a frame holding no entry between two that hold some is read as the concatenation.
#[test]
fn test_a_frame_of_no_entry_between_two_reads() -> TestResult {
    let answer = taken(&[
        part(&["urn:a"], false)?,
        part(&[], false)?,
        part(&["urn:b"], true)?,
    ])?
    .ok()
    .context("the answer")?;
    assert_eq!(answer.predicates(), owned(&["urn:a", "urn:b"]));
    // The order still runs on across the empty frame.
    let refusal = taken(&[
        part(&["urn:b"], false)?,
        part(&[], false)?,
        part(&["urn:a"], true)?,
    ])?
    .err()
    .context("a refusal")?;
    assert_eq!(refusal, field(539, "default order", 1));
    Ok(())
}

/// C1.66, C1.67: a second frame whose first entry equals the first frame's last, or lies below it, is refused naming
/// the entry's index in the answer.
#[test]
fn test_the_order_runs_on_across_frames() -> TestResult {
    for first_of_second in ["urn:b", "urn:a"] {
        let refusal = taken(&[
            part(&["urn:a", "urn:b"], false)?,
            part(&[first_of_second, "urn:z"], true)?,
        ])?
        .err()
        .context("a refusal")?;
        assert_eq!(refusal, field(539, "default order", 2));
    }
    Ok(())
}

/// An entry's index runs on across the frames for an entry out of order inside a later frame too.
#[test]
fn test_an_entrys_index_runs_on_inside_a_later_frame() -> TestResult {
    let refusal = taken(&[
        part(&["urn:a", "urn:b"], false)?,
        part(&["urn:c", "urn:e", "urn:d"], true)?,
    ])?
    .err()
    .context("a refusal")?;
    assert_eq!(refusal, field(539, "default order", 4));
    Ok(())
}

/// C1.68: a frame breaking the first's command header, and a frame after the one flagged `LAST`, is refused naming
/// the frames taken before it.
#[test]
fn test_a_frame_breaking_the_first_command_header_is_refused() -> TestResult {
    let first = part(&["urn:a"], false)?;
    let other_tenant = defaults_frame(4, 7, &iris(&["urn:b"]), true)?;
    let other_position = defaults_frame(3, 8, &iris(&["urn:b"]), true)?;
    for second in [other_tenant, other_position] {
        let refusal = taken(&[first.clone(), second])?
            .err()
            .context("a refusal")?;
        assert_eq!(refusal, field(539, "defaults", 1));
    }
    let after_last = taken(&[
        part(&["urn:a"], false)?,
        part(&["urn:b"], true)?,
        part(&["urn:c"], true)?,
    ])?
    .err()
    .context("a refusal")?;
    assert_eq!(after_last, field(539, "defaults", 2));
    Ok(())
}

/// C1.68, the order decided: a continued frame's own rules come before the rule tying it to the first frame. A second
/// frame of `position` 0 holding an entry breaks both, and is refused as the request it reads as; one out of order
/// inside itself and of another tenant is refused for its order.
#[test]
fn test_a_frames_own_rules_come_before_the_first_frames_header() -> TestResult {
    let request_with_entry = defaults_frame(3, 0, &iris(&["urn:b"]), true)?;
    let refusal = taken(&[part(&["urn:a"], false)?, request_with_entry])?
        .err()
        .context("a refusal")?;
    assert_eq!(refusal, count(539, "defaults", 1, 0));
    let unordered_other_tenant = defaults_frame(4, 7, &iris(&["urn:c", "urn:b"]), true)?;
    let refusal_2 = taken(&[part(&["urn:a"], false)?, unordered_other_tenant])?
        .err()
        .context("a refusal")?;
    assert_eq!(refusal_2, field(539, "default order", 2));
    // The first frame's header comes before the order across frames.
    let below_other_tenant = defaults_frame(4, 7, &iris(&["urn:a"]), true)?;
    let refusal_3 = taken(&[part(&["urn:b"], false)?, below_other_tenant])?
        .err()
        .context("a refusal")?;
    assert_eq!(refusal_3, field(539, "defaults", 1));
    Ok(())
}

/// C1.69: once the term tables' bytes pass the bound the requester reads no further. A predicate of 5 bytes takes 29
/// bytes of term table, a record of 24 and its text: under a bound of 58 two frames of one predicate are read, and the
/// third is not, nor any after it, a frame its own rules would refuse included.
#[test]
fn test_no_frame_is_read_once_the_answer_passes_its_bound() -> TestResult {
    let mut taken = DefaultsFrames::with_bytes_max(58);
    taken.push(&frame_of(&part(&["urn:a"], false)?)?)?;
    taken.push(&frame_of(&part(&["urn:b"], false)?)?)?;
    assert!(!taken.is_past_bound());
    assert_eq!(taken.predicates(), owned(&["urn:a", "urn:b"]));
    taken.push(&frame_of(&part(&["urn:c"], false)?)?)?;
    assert!(taken.is_past_bound());
    assert!(!taken.is_last());
    assert_eq!(taken.predicates(), owned(&["urn:a", "urn:b"]));
    // No further frame is read: one out of order, and one flagged `LAST`.
    taken.push(&frame_of(&part(&["urn:a"], false)?)?)?;
    taken.push(&frame_of(&part(&["urn:d"], true)?)?)?;
    assert!(taken.is_past_bound());
    assert!(!taken.is_last());
    assert_eq!(taken.predicates(), owned(&["urn:a", "urn:b"]));
    Ok(())
}

/// The bound a requester takes by default is `catalog_bytes_max`: an answer of the golden reply is far within it.
#[test]
fn test_the_default_bound_is_catalog_bytes_max() -> TestResult {
    let mut within = DefaultsFrames::with_bytes_max(CATALOG_BYTES_MAX);
    within.push(&frame_of(&role("defaults_reply")?)?)?;
    let mut default = DefaultsFrames::new();
    default.push(&frame_of(&role("defaults_reply")?)?)?;
    assert_eq!(within, default);
    assert!(!default.is_past_bound());
    // The golden reply's term table is 72 bytes of records and 53 of text: a bound of 125 reads it, one of 124 does not.
    let mut exact = DefaultsFrames::with_bytes_max(125);
    exact.push(&frame_of(&role("defaults_reply")?)?)?;
    assert!(!exact.is_past_bound() && exact.is_last());
    let mut short = DefaultsFrames::with_bytes_max(124);
    short.push(&frame_of(&role("defaults_reply")?)?)?;
    assert!(short.is_past_bound() && !short.is_last());
    assert!(short.predicates().is_empty());
    Ok(())
}
