/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Entries, EntriesEnd, EntriesSequence, FLAG_LAST, Header,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};

const KEY: &[u8] = b"\x10k";
const BELOW: &[u8] = b"\x10j";
const ABOVE: &[u8] = b"\x10l";

/// The receiver's sequence of an answer to a Scan of `ranges` ranges.
struct Answer {
    ranges: u32,
    sequence: EntriesSequence,
}

impl Answer {
    fn new(ranges: u32) -> Self {
        Self {
            ranges,
            sequence: EntriesSequence::new(ranges),
        }
    }

    /// The sequence given the frame of `entries`, carrying `LAST` as `last` says.
    fn flagged(
        &mut self,
        entries: &Entries,
        last: bool,
    ) -> TestResult<Result<(), ProtocolErrorEnum>> {
        let flags = if last { FLAG_LAST } else { 0 };
        let header = Header::new(CLUSTER, Command::Entries).with_flags(flags);
        let bytes = entries.frame(header)?;
        let frame = frame_of(&bytes)?;
        Ok(self.sequence.accept(&frame).map(|_| ()).map_err(|e| e.0))
    }

    /// The sequence given the frame of `entries`, carrying `LAST` where the protocol has it: the last range's frame with
    /// `end` not 0.
    fn offer(&mut self, entries: &Entries) -> TestResult<Result<(), ProtocolErrorEnum>> {
        let last =
            entries.range().saturating_add(1) == self.ranges && entries.end() != EntriesEnd::More;
        self.flagged(entries, last)
    }

    /// The sequence taking the frame of `entries`.
    fn take(&mut self, entries: &Entries) -> TestResult {
        assert_eq!(self.offer(entries)?, Ok(()));
        Ok(())
    }
}

/// `entries` with `keys` as its entries, each with an empty value.
fn holding(mut entries: Entries, keys: &[&[u8]]) -> Entries {
    keys.iter().for_each(|key| entries.push(key, b""));
    entries
}

/// An answer of `ranges` ranges having taken range 0's batch of [`KEY`], ending as `end` says.
fn after(ranges: u32, end: EntriesEnd) -> TestResult<Answer> {
    let mut answer = Answer::new(ranges);
    answer.take(&holding(Entries::new(0, end, 0), &[KEY]))?;
    Ok(answer)
}

/// An answer having taken range 0's batch of [`KEY`], its value continuing.
fn after_continues() -> TestResult<Answer> {
    let mut answer = Answer::new(1);
    answer.take(&holding(Entries::continuing(0, 0), &[KEY]))?;
    Ok(answer)
}

fn refused(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 525,
        field,
        value,
    }
}

fn entry_order() -> ProtocolErrorEnum {
    refused("entry order", 0)
}

fn range(value: u64) -> ProtocolErrorEnum {
    refused("range", value)
}

#[test]
fn test_the_golden_frames_across_a_list_are_accepted_and_read() -> TestResult {
    let mut sequence = EntriesSequence::new(1);
    for name in ["entries_continues", "entries_continued"] {
        let bytes = role_bytes(&golden_role(name)?)?;
        let view = sequence.accept(&frame_of(&bytes)?)?;
        assert_eq!(view.range(), 0, "{name}");
    }
    Ok(())
}

/// What the frame's own view refuses, the sequence refuses, before comparing it with any frame.
#[test]
fn test_a_frame_its_view_refuses_is_refused() -> TestResult {
    let mut answer = Answer::new(2);
    assert_eq!(
        answer.offer(&Entries::continuing(1, 0))?,
        Err(refused("continues", 1))
    );
    Ok(())
}

#[test]
fn test_a_batchs_first_key_lies_above_the_ranges_last() -> TestResult {
    for key in [KEY, BELOW] {
        let batch = holding(Entries::new(0, EntriesEnd::More, 0), &[key]);
        assert_eq!(
            after(1, EntriesEnd::More)?.offer(&batch)?,
            Err(entry_order())
        );
    }
    let batch = holding(Entries::new(0, EntriesEnd::Ended, 0), &[ABOVE]);
    assert_eq!(after(1, EntriesEnd::More)?.offer(&batch)?, Ok(()));
    Ok(())
}

#[test]
fn test_the_last_key_is_the_batchs_last_not_its_first() -> TestResult {
    let mut answer = Answer::new(1);
    answer.take(&holding(
        Entries::new(0, EntriesEnd::More, 0),
        &[BELOW, KEY],
    ))?;
    let next = holding(Entries::new(0, EntriesEnd::More, 0), &[KEY]);
    assert_eq!(answer.offer(&next)?, Err(entry_order()));
    Ok(())
}

#[test]
fn test_after_continues_the_first_key_is_the_last_key() -> TestResult {
    for key in [BELOW, ABOVE] {
        let batch = holding(Entries::new(0, EntriesEnd::More, 0), &[key]);
        assert_eq!(after_continues()?.offer(&batch)?, Err(entry_order()));
    }
    let same = holding(Entries::new(0, EntriesEnd::Ended, 0), &[KEY, ABOVE]);
    assert_eq!(after_continues()?.offer(&same)?, Ok(()));
    let again = holding(Entries::continuing(0, 0), &[KEY]);
    assert_eq!(after_continues()?.offer(&again)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_continuation_taken_returns_to_ascending_keys() -> TestResult {
    let mut answer = after_continues()?;
    let same = holding(Entries::new(0, EntriesEnd::More, 0), &[KEY]);
    answer.take(&same)?;
    assert_eq!(answer.offer(&same)?, Err(entry_order()));
    Ok(())
}

#[test]
fn test_an_empty_batch_keeps_the_ranges_last_key() -> TestResult {
    let mut answer = after(1, EntriesEnd::More)?;
    answer.take(&Entries::new(0, EntriesEnd::More, 0))?;
    let batch = holding(Entries::new(0, EntriesEnd::More, 0), &[KEY]);
    assert_eq!(answer.offer(&batch)?, Err(entry_order()));
    Ok(())
}

/// A value promised to continue cannot stop: the frame after `continues` 1 holds an entry, however it ends.
#[test]
fn test_an_empty_batch_after_continues_is_refused() -> TestResult {
    for end in [EntriesEnd::More, EntriesEnd::Ended, EntriesEnd::Limit] {
        assert_eq!(
            after_continues()?.offer(&Entries::new(0, end, 0))?,
            Err(entry_order())
        );
    }
    Ok(())
}

/// Clause 1 reads the frame naming the previous frame's range: a frame naming another range answers to clause 2, whatever the
/// frame before promised.
#[test]
fn test_a_promise_to_continue_binds_only_the_frame_of_its_range() -> TestResult {
    let mut answer = Answer::new(2);
    let mut first = Entries::continuing(0, 0);
    first.push(KEY, b"v");
    assert_eq!(answer.flagged(&first, false)?, Ok(()));
    let other = Entries::new(1, EntriesEnd::Ended, 0);
    let refused = answer.flagged(&other, true)?.err();
    assert_eq!(
        refused,
        Some(ProtocolErrorEnum::FieldValue {
            command: Command::Entries.number(),
            field: "range",
            value: 1,
        })
    );
    Ok(())
}

#[test]
fn test_a_new_range_has_no_last_key() -> TestResult {
    let mut answer = after(2, EntriesEnd::Ended)?;
    let batch = holding(Entries::new(1, EntriesEnd::More, 0), &[BELOW]);
    assert_eq!(answer.offer(&batch)?, Ok(()));
    Ok(())
}

#[test]
fn test_the_first_frame_is_range_0() -> TestResult {
    let batch = Entries::new(1, EntriesEnd::Ended, 0);
    assert_eq!(
        Answer::new(2).offer(&batch)?,
        Err(range(batch.range().into()))
    );
    Ok(())
}

#[test]
fn test_end_0_stays_in_its_range() -> TestResult {
    for other in [1u32, 2] {
        let batch = Entries::new(other, EntriesEnd::Ended, 0);
        assert_eq!(
            after(3, EntriesEnd::More)?.offer(&batch)?,
            Err(range(other.into()))
        );
    }
    Ok(())
}

#[test]
fn test_an_ended_range_moves_to_the_next() -> TestResult {
    for end in [EntriesEnd::Ended, EntriesEnd::Limit] {
        for other in [0u32, 2] {
            let batch = Entries::new(other, EntriesEnd::Ended, 0);
            assert_eq!(after(3, end)?.offer(&batch)?, Err(range(other.into())));
        }
        let next = Entries::new(1, EntriesEnd::Ended, 0);
        assert_eq!(after(3, end)?.offer(&next)?, Ok(()));
    }
    Ok(())
}

#[test]
fn test_a_range_lies_below_the_scans_count() -> TestResult {
    let batch = Entries::new(0, EntriesEnd::Ended, 0);
    assert_eq!(Answer::new(0).offer(&batch)?, Err(range(0)));
    let past = Entries::new(1, EntriesEnd::Ended, 0);
    assert_eq!(after(1, EntriesEnd::Ended)?.offer(&past)?, Err(range(1)));
    Ok(())
}

#[test]
fn test_last_on_any_frame_but_the_last_ranges_ending_is_refused() -> TestResult {
    // An earlier range's ending frame.
    let ended = Entries::new(0, EntriesEnd::Ended, 0);
    assert_eq!(
        Answer::new(2).flagged(&ended, true)?,
        Err(refused("end", 0))
    );
    let stopped = Entries::new(0, EntriesEnd::Limit, 0);
    assert_eq!(
        Answer::new(2).flagged(&stopped, true)?,
        Err(refused("end", 0))
    );
    // The last range's frame with more of it to come.
    let more = Entries::new(1, EntriesEnd::More, 0);
    assert_eq!(
        after(2, EntriesEnd::Ended)?.flagged(&more, true)?,
        Err(refused("end", more.range().into()))
    );
    Ok(())
}

#[test]
fn test_the_last_ranges_ending_frame_carries_last() -> TestResult {
    for end in [EntriesEnd::Ended, EntriesEnd::Limit] {
        let last = Entries::new(1, end, 0);
        assert_eq!(
            after(2, EntriesEnd::Ended)?.flagged(&last, false)?,
            Err(refused("end", last.range().into()))
        );
        assert_eq!(after(2, EntriesEnd::Ended)?.flagged(&last, true)?, Ok(()));
    }
    Ok(())
}

/// Nothing follows the last range's ending frame: its range is past the Scan's count.
#[test]
fn test_no_frame_follows_the_answers_last() -> TestResult {
    let mut answer = after(1, EntriesEnd::Ended)?;
    let after_last = Entries::new(0, EntriesEnd::Ended, 0);
    assert_eq!(answer.flagged(&after_last, true)?, Err(range(0)));
    Ok(())
}

#[test]
fn test_the_key_rule_comes_before_the_range_rule() -> TestResult {
    let batch = holding(Entries::new(0, EntriesEnd::More, 0), &[BELOW]);
    assert_eq!(
        after(2, EntriesEnd::Ended)?.offer(&batch)?,
        Err(entry_order())
    );
    let above = holding(Entries::new(0, EntriesEnd::More, 0), &[ABOVE]);
    assert_eq!(after(2, EntriesEnd::Ended)?.offer(&above)?, Err(range(0)));
    Ok(())
}

#[test]
fn test_the_range_rule_comes_before_the_last_rule() -> TestResult {
    let batch = Entries::new(1, EntriesEnd::Ended, 0);
    assert_eq!(Answer::new(2).flagged(&batch, false)?, Err(range(1)));
    Ok(())
}

#[test]
fn test_the_key_rule_comes_before_the_last_rule() -> TestResult {
    let batch = holding(Entries::new(0, EntriesEnd::Ended, 0), &[BELOW]);
    assert_eq!(
        after(1, EntriesEnd::More)?.flagged(&batch, false)?,
        Err(entry_order())
    );
    Ok(())
}
