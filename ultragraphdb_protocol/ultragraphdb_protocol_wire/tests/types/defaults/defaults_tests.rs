/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Defaults' builder (`PROTOCOL.md` §13.28) against the golden `defaults.frame` and `defaults_reply.frame` and frames
//! laid out by hand; it refuses what its view and `DefaultsFrames` refuse.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Defaults, FLAG_LAST, Header};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::system_frames::{count, defaults_frame, field, iris};

/// The golden reply's predicates, in the order of their texts' bytes: an upper-case letter before a lower-case one.
pub const GOLDEN_PREDICATES: [&str; 3] = [
    "http://ex.org/Zeta",
    "http://ex.org/age",
    "http://ex.org/name",
];

/// `texts` as owned predicates.
pub fn owned(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_string()).collect()
}

/// The header `utils::ugs_frames::frame` lays its frames out under.
pub fn header() -> Header {
    Header::new(CLUSTER, Command::Defaults).with_stream(3)
}

fn refused(defaults: &Defaults) -> TestResult<ProtocolErrorEnum> {
    Ok(defaults.frame(header()).err().context("a refusal")?.0)
}

/// C1.76: the builder lays out both golden frames.
#[test]
fn test_a_defaults_builds_the_golden_request_and_reply() -> TestResult {
    let request = Defaults::request(3);
    assert_eq!((request.tenant(), request.position()), (3, 0));
    assert!(request.predicates().is_empty());
    let golden = golden_role("defaults")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    let reply = Defaults::reply(3, 1_250, owned(&GOLDEN_PREDICATES));
    assert_eq!((reply.tenant(), reply.position()), (3, 1_250));
    assert_eq!(reply.predicates(), owned(&GOLDEN_PREDICATES).as_slice());
    let golden_reply = golden_role("defaults_reply")?;
    assert_eq!(
        reply.frame(golden_reply.header())?,
        role_bytes(&golden_reply)?
    );
    Ok(())
}

/// C1.70, C1.71 at the builder: a reply of no predicate and of one, against the frame laid out by hand.
#[test]
fn test_a_reply_of_no_predicate_and_of_one_is_laid_out() -> TestResult {
    assert_eq!(
        Defaults::reply(7, 1, Vec::new()).frame(header())?,
        defaults_frame(7, 1, &[], false)?
    );
    assert_eq!(
        Defaults::reply(7, u64::MAX, owned(&["urn:p"])).frame(header())?,
        defaults_frame(7, u64::MAX, &iris(&["urn:p"]), false)?
    );
    Ok(())
}

/// C1.62 at the builder: a body of `position` 0 is a request, which holds no predicate.
#[test]
fn test_the_builder_refuses_a_predicate_in_a_request() -> TestResult {
    let request = Defaults::reply(3, 0, owned(&["urn:p"]));
    assert_eq!(refused(&request)?, count(539, "defaults", 1, 0));
    Ok(())
}

/// C1.64, C1.65 at the builder: a predicate below the one before is refused naming its index, and one repeated in the
/// frame as the term table refuses it, which is checked first.
#[test]
fn test_the_builder_refuses_predicates_out_of_order() -> TestResult {
    let below = Defaults::reply(3, 1, owned(&["urn:a", "urn:c", "urn:b"]));
    assert_eq!(refused(&below)?, field(539, "default order", 2));
    let twice = Defaults::reply(3, 1, owned(&["urn:b", "urn:a", "urn:b"]));
    assert_eq!(
        refused(&twice)?,
        ProtocolErrorEnum::DuplicateTerm {
            command: 539,
            section: 0,
            at: 2,
            earlier: 0
        }
    );
    Ok(())
}

/// The frame a Defaults reply of `texts`, each `len` bytes long, takes: 256 bytes of header, a two-entry section
/// table, a record of 24 bytes and the text a predicate (§2.3, §4.1, §4.4).
fn frame_len(count: usize, len: usize) -> usize {
    256 + 16 + count * (24 + len)
}

/// Predicates of 16 bytes each, ascending.
fn predicates(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("urn:pred:{index:07}"))
        .collect()
}

/// C1.73: an answer that fits is one frame flagged `LAST`, the frame laid out by hand.
#[test]
fn test_an_answer_that_fits_is_one_frame() -> TestResult {
    let texts = predicates(3);
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let reply = Defaults::reply(5, 9, texts.clone());
    let expected = defaults_frame(5, 9, &iris(&refs), true)?;
    for max_frame in [frame_len(3, 16), 1 << 20] {
        assert_eq!(reply.frames(header(), max_frame)?, vec![expected.clone()]);
    }
    // A reply of no predicate is one frame too.
    assert_eq!(
        Defaults::reply(5, 9, Vec::new()).frames(header(), 1 << 20)?,
        vec![defaults_frame(5, 9, &[], true)?]
    );
    Ok(())
}

/// C1.74: an answer past one frame continues: each frame holds a whole term table of as many predicates as fit and
/// the first's command header, the last alone flagged `LAST`. Cut at two predicates a frame, at three, and at one.
#[test]
fn test_an_answer_past_max_frame_continues_in_whole_term_tables() -> TestResult {
    let texts = predicates(5);
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let reply = Defaults::reply(5, 9, texts.clone());
    for per_frame in [1usize, 2, 3] {
        // One byte short of the next predicate: the cut falls where the count says.
        let max_frame = frame_len(per_frame + 1, 16) - 1;
        let frames = reply.frames(header(), max_frame)?;
        let parts: Vec<&[&str]> = refs.chunks(per_frame).collect();
        assert_eq!(frames.len(), parts.len());
        for (index, (bytes, part)) in frames.iter().zip(&parts).enumerate() {
            let last = index + 1 == parts.len();
            assert!(bytes.len() <= max_frame);
            assert_eq!(bytes, &defaults_frame(5, 9, &iris(part), last)?);
            let view = frame_of(bytes)?;
            assert_eq!(view.header().flags() & FLAG_LAST == FLAG_LAST, last);
        }
    }
    Ok(())
}

/// A frame too small for the next predicate is refused naming the least frame that holds it.
#[test]
fn test_a_frame_too_small_for_one_predicate_is_refused() -> TestResult {
    let reply = Defaults::reply(5, 9, predicates(2));
    let least = frame_len(1, 16);
    let refusal = reply
        .frames(header(), least - 1)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ProtocolErrorEnum::FrameTooLarge {
            size: least,
            limit: least - 1
        }
    );
    assert_eq!(reply.frames(header(), least)?.len(), 2);
    Ok(())
}

/// C1.66, C1.67 at the builder: the order runs on across the frames it cuts, a predicate at or below the one before
/// refused naming its index in the answer, whichever frame it falls in.
#[test]
fn test_frames_refuses_predicates_out_of_order_across_its_frames() -> TestResult {
    let one_a_frame = frame_len(1, 5);
    for texts in [["urn:a", "urn:b", "urn:b"], ["urn:a", "urn:c", "urn:b"]] {
        let refusal = Defaults::reply(3, 1, owned(&texts))
            .frames(header(), one_a_frame)
            .err()
            .context("a refusal")?;
        assert_eq!(refusal.0, field(539, "default order", 2));
    }
    let request = Defaults::reply(3, 0, owned(&["urn:p"]));
    let refusal = request
        .frames(header(), 1 << 20)
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, count(539, "defaults", 1, 0));
    Ok(())
}

/// Phase 5: the frames' `LAST` is the builder's whatever the header given holds: under a header flagged `LAST` the
/// frames before the last carry none, and the last carries it once.
#[test]
fn test_frames_flags_the_last_frame_alone_under_a_header_flagged_last() -> TestResult {
    let reply = Defaults::reply(5, 9, predicates(3));
    let flagged = header().with_flags(FLAG_LAST);
    let frames = reply.frames(flagged.clone(), frame_len(1, 16))?;
    let flags: Vec<u8> = frames
        .iter()
        .map(|bytes| frame_of(bytes).map(|view| view.header().flags()))
        .collect::<TestResult<_>>()?;
    assert_eq!(flags, [0, 0, FLAG_LAST]);
    let one = reply.frames(flagged, 1 << 20)?;
    assert_eq!(one.len(), 1);
    assert_eq!(frame_of(&one[0])?.header().flags(), FLAG_LAST);
    Ok(())
}
