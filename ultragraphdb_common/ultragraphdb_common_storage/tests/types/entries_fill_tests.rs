/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use ultragraphdb_common_storage::EntriesFill;
use ultragraphdb_core_ids::{Bucket, Epoch, Sequence, SystemId, TypePrefix, Version};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Entries, EntriesEnd, EntriesView, FrameView, Header,
    MAX_FRAME_SIZE, ObjectRecords,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 1;
const KEY: &[u8] = b"\x10a list";
const NEXT: &[u8] = b"\x10b list";

/// The epoch and version every frame here names.
fn at() -> (u64, u64) {
    (7, Version::bound_of(Epoch::ZERO))
}

fn object(sequence: u64) -> TestResult<(EncodedTerm, SystemId)> {
    let id = SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(1)?,
        Sequence::new(sequence)?,
    );
    Ok((EncodedTerm::id(id), id))
}

/// The object numbered `sequence` and the `statements` statement IDs it is held under.
fn object_held(sequence: u64, statements: u64) -> TestResult<(EncodedTerm, Vec<SystemId>)> {
    let (term, _) = object(sequence)?;
    let held = (1..=statements)
        .map(|at| {
            Ok(SystemId::new(
                TypePrefix::STATEMENT,
                Bucket::new(1)?,
                Sequence::new(at)?,
            ))
        })
        .collect::<TestResult<_>>()?;
    Ok((term, held))
}

fn built(entries: &Entries) -> TestResult<Vec<u8>> {
    Ok(entries.frame(Header::new(CLUSTER, Command::Entries).with_stream(1))?)
}

/// `count` objects under one key, as a frame with no limit holds them: its bytes.
fn frame_of(count: u64) -> TestResult<usize> {
    frame_held_under(count, 0)
}

/// `count` objects under one key, each held under `statements` statements, as a frame with no limit holds them: its bytes.
fn frame_held_under(count: u64, statements: u64) -> TestResult<usize> {
    let mut fill = EntriesFill::new(0, at(), usize::MAX);
    for at in 1..=count {
        let (term, held) = object_held(at, statements)?;
        assert!(fill.offer_object(KEY, &term, &held).is_continue());
    }
    Ok(built(&fill.finish(EntriesEnd::More))?.len())
}

/// A frame of at most `max_frame` bytes filled with objects held under `statements` statements each until it refuses one: how
/// many it took. A record is at least a byte, so a frame taking more than `max_frame` of them took past its limit.
fn filled(max_frame: usize, statements: u64) -> TestResult<(u64, usize)> {
    let mut fill = EntriesFill::new(0, at(), max_frame);
    for taken in 0..=u64::try_from(max_frame)? {
        let (term, held) = object_held(taken + 1, statements)?;
        if fill.offer_object(KEY, &term, &held).is_break() {
            return Ok((taken, built(&fill.finish(EntriesEnd::More))?.len()));
        }
    }
    Err(TestError::new(format!(
        "{max_frame}: no object was refused"
    )))
}

/// A frame takes what fits and refuses the next, by the exact bytes it builds, on either side of the chunk table's growth.
#[test]
fn test_a_frame_is_filled_to_its_limit_and_no_further() -> TestResult {
    let one = frame_of(1)?;
    for max_frame in [one, one + 24, one + 25, one + 4_096, one + 70_000, 1 << 19] {
        let (taken, frame) = filled(max_frame, 0)?;
        assert!(frame <= max_frame, "{max_frame}");
        assert_eq!(frame, frame_of(taken)?, "the size it was held to");
        assert!(
            frame_of(taken + 1)? > max_frame,
            "{max_frame}: room was left"
        );
    }
    Ok(())
}

/// Each statement ID an object is held under counts toward its record: a frame of such objects is filled to its limit and no
/// further.
#[test]
fn test_an_objects_statement_ids_count_toward_its_frame() -> TestResult {
    for statements in [1, 2, 5] {
        let one = frame_held_under(1, statements)?;
        for max_frame in [one, one + 100, one + 4_096] {
            let (taken, frame) = filled(max_frame, statements)?;
            assert!(frame <= max_frame, "{statements} under {max_frame}");
            assert_eq!(frame, frame_held_under(taken, statements)?);
            assert!(
                frame_held_under(taken + 1, statements)? > max_frame,
                "{statements} under {max_frame}: room was left"
            );
        }
    }
    Ok(())
}

/// One object larger than an empty frame is refused with nothing taken, which the core refuses as too large.
#[test]
fn test_an_object_too_large_for_an_empty_frame_is_not_taken() -> TestResult {
    let (term, _) = object(1)?;
    let mut fill = EntriesFill::new(0, at(), 8);
    assert_eq!(fill.offer_object(KEY, &term, &[]), ControlFlow::Break(()));
    assert!(fill.finish(EntriesEnd::More).entries().is_empty());
    let mut stored = EntriesFill::new(0, at(), 8);
    assert_eq!(stored.offer_value(KEY, b"value"), ControlFlow::Break(()));
    Ok(())
}

/// A frame reads back as its objects under one key each, with their statement IDs, in the order offered.
#[test]
fn test_a_frame_holds_each_key_once_with_its_objects() -> TestResult {
    let (first, statement) = object(1)?;
    let (second, _) = object(2)?;
    let mut fill = EntriesFill::new(2, at(), 1 << 20);
    assert!(fill.offer_object(KEY, &first, &[statement]).is_continue());
    assert!(fill.offer_object(KEY, &second, &[]).is_continue());
    assert!(fill.offer_object(NEXT, &first, &[]).is_continue());
    let frame = built(&fill.finish(EntriesEnd::Limit))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let entries = EntriesView::from_frame(&view)?;
    assert_eq!(
        (entries.range(), entries.end(), entries.epoch()),
        (2, EntriesEnd::Limit, 7)
    );
    let held: Vec<_> = entries.entries().iter().collect();
    let [one, two] = held.as_slice() else {
        return Err(TestError::new("two keys"));
    };
    assert_eq!((one.key(), two.key()), (KEY, NEXT));
    let records = ObjectRecords::read(one.value().context("a value")?)?;
    let read: Vec<_> = records.iter().collect();
    assert_eq!(read.len(), 2);
    assert_eq!(
        read.first().context("the first")?.encoding(),
        first.as_bytes()
    );
    assert_eq!(
        read.first()
            .context("the first")?
            .statement_ids()
            .collect::<Vec<_>>(),
        [statement.to_be_bytes()]
    );
    assert_eq!(
        read.get(1).context("the second")?.encoding(),
        second.as_bytes()
    );
    Ok(())
}

/// A frame stopping inside a key says its value continues.
#[test]
fn test_a_frame_stopping_inside_a_key_continues() -> TestResult {
    let (term, _) = object(1)?;
    let mut fill = EntriesFill::new(0, at(), 1 << 20);
    assert!(fill.offer_object(KEY, &term, &[]).is_continue());
    let entries = fill.finish_continuing();
    assert!(entries.continues());
    assert_eq!(entries.end(), EntriesEnd::More);
    assert_eq!(entries.entries().len(), 1);
    Ok(())
}

/// A run's frame names its last range and how each ended, and every frame of a fill names the version it was given.
#[test]
fn test_a_run_names_its_ranges_through_its_last() {
    let mut fill = EntriesFill::new(2, at(), MAX_FRAME_SIZE);
    assert!(fill.offer_value(KEY, b"id").is_continue());
    let entries = fill.finish_run(5);
    assert_eq!(
        (entries.range(), entries.through(), entries.end()),
        (2, 5, EntriesEnd::Ended)
    );
    assert_eq!((entries.epoch(), entries.version()), at());
}

/// A frame holding something stops at what its turn may yet build, and one holding nothing takes a first record up to `max_frame`, so
/// a turn nearly spent still moves its Scan on.
#[test]
fn test_a_frame_stops_at_its_turn_and_takes_a_first_record() -> TestResult {
    let whole = frame_of(1)?;
    let two = frame_of(2)?;
    let (first, _) = object(1)?;
    let (second, _) = object(2)?;
    let mut smaller = EntriesFill::new(0, at(), MAX_FRAME_SIZE).filling_to(whole - 1);
    assert!(smaller.offer_object(KEY, &first, &[]).is_continue());
    assert!(smaller.offer_object(KEY, &second, &[]).is_break());
    let mut holds_two = EntriesFill::new(0, at(), MAX_FRAME_SIZE).filling_to(two);
    assert!(holds_two.offer_object(KEY, &first, &[]).is_continue());
    assert!(holds_two.offer_object(KEY, &second, &[]).is_continue());
    let mut past_max = EntriesFill::new(0, at(), whole - 1).filling_to(1);
    assert!(past_max.offer_object(KEY, &first, &[]).is_break());
    Ok(())
}
