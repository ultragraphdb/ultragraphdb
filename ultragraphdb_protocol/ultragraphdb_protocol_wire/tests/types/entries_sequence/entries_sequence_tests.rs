/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Entries, EntriesEnd, EntriesSequence, FLAG_LAST, Header, ScanRange,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};

const KEY: &[u8] = b"\x10k";
const BELOW: &[u8] = b"\x10j";
const ABOVE: &[u8] = b"\x10l";

/// `count` ranges of the keyspace [`KEY`] lies in, each holding every key of it.
fn whole(count: u32) -> Vec<ScanRange<'static>> {
    (0..count)
        .map(|_| ScanRange::new(1, b"\x10", b""))
        .collect()
}

/// The receiver's sequence of an answer to a Scan of `ranges` ranges.
struct Answer {
    ranges: u32,
    sequence: EntriesSequence,
}

impl Answer {
    fn new(ranges: u32) -> Self {
        Self {
            ranges,
            sequence: EntriesSequence::new(whole(ranges)),
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
    let mut sequence = EntriesSequence::new(whole(1));
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

/// The dictionary keys of a run's four ranges, and the range holding each alone.
const RUN: [&[u8]; 4] = [b"\x1b\x04", b"\x1b\x03", b"\x1b\x02", b"\x1b\x01"];

/// A sequence of a Scan of [`RUN`]'s one-key ranges.
fn run_sequence() -> EntriesSequence {
    let pasts: Vec<Vec<u8>> = RUN.iter().map(|key| [*key, &[0]].concat()).collect();
    EntriesSequence::new(
        RUN.iter()
            .zip(&pasts)
            .map(|(key, past)| ScanRange::new(1, key, past)),
    )
}

/// What `sequence` makes of the frame of `entries`, carrying `LAST` as `last` says.
fn offered(
    sequence: &mut EntriesSequence,
    entries: &Entries,
    last: bool,
) -> TestResult<Result<(), ProtocolErrorEnum>> {
    let flags = if last { FLAG_LAST } else { 0 };
    let bytes = entries.frame(Header::new(CLUSTER, Command::Entries).with_flags(flags))?;
    let frame = frame_of(&bytes)?;
    Ok(sequence.accept(&frame).map(|_| ()).map_err(|e| e.0))
}

/// A run's entries follow its ranges' order, not the keys', and a range finding nothing holds no entry.
#[test]
fn test_a_run_is_taken_in_its_ranges_order() -> TestResult {
    let mut sequence = run_sequence();
    let mut whole = Entries::new(0, EntriesEnd::Ended, 0).with_through(3);
    [RUN[0], RUN[2], RUN[3]]
        .iter()
        .for_each(|key| whole.push(key, b"id"));
    assert_eq!(offered(&mut sequence, &whole, true)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_run_leads_to_the_range_after_its_through() -> TestResult {
    let mut sequence = run_sequence();
    let first = Entries::new(0, EntriesEnd::Ended, 0).with_through(1);
    assert_eq!(offered(&mut sequence, &first, false)?, Ok(()));
    let next = Entries::new(2, EntriesEnd::Ended, 0).with_through(3);
    assert_eq!(offered(&mut sequence, &next, true)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_range_inside_the_last_run_is_refused() -> TestResult {
    let mut sequence = run_sequence();
    let first = Entries::new(0, EntriesEnd::Ended, 0).with_through(1);
    assert_eq!(offered(&mut sequence, &first, false)?, Ok(()));
    let inside = Entries::new(1, EntriesEnd::Ended, 0).with_through(3);
    assert_eq!(offered(&mut sequence, &inside, true)?, Err(range(1)));
    Ok(())
}

#[test]
fn test_a_through_lies_below_the_scans_count() -> TestResult {
    let past = Entries::new(0, EntriesEnd::Ended, 0).with_through(4);
    assert_eq!(offered(&mut run_sequence(), &past, true)?, Err(range(0)));
    Ok(())
}

/// A run covers one-key ranges of the dictionary alone: a range of another keyspace, or of more than one key, is none.
#[test]
fn test_a_run_covers_one_key_dictionary_ranges_alone() -> TestResult {
    let run = Entries::new(0, EntriesEnd::Ended, 0).with_through(1);
    let keyed = |first: &'static [u8], past: &'static [u8]| ScanRange::new(1, first, past);
    for other in [
        keyed(b"\x10k", b"\x10k\x00"),
        keyed(b"\x1b\x01", b"\x1b\x02"),
        keyed(b"\x1b\x01", b""),
    ] {
        let mut sequence = EntriesSequence::new([keyed(b"\x1b\x00", b"\x1b\x00\x00"), other]);
        assert_eq!(offered(&mut sequence, &run, true)?, Err(range(0)));
    }
    Ok(())
}

#[test]
fn test_last_goes_on_the_run_reaching_the_last_range() -> TestResult {
    let reaching = Entries::new(0, EntriesEnd::Ended, 0).with_through(3);
    assert_eq!(
        offered(&mut run_sequence(), &reaching, false)?,
        Err(refused("end", 0))
    );
    let short = Entries::new(0, EntriesEnd::Ended, 0).with_through(2);
    assert_eq!(
        offered(&mut run_sequence(), &short, true)?,
        Err(refused("end", 0))
    );
    Ok(())
}

#[test]
fn test_every_frame_names_the_first_frames_version() -> TestResult {
    let bound = (30 << 22) | 0x3F_FFFF;
    let mut answer = Answer::new(1);
    answer.take(&Entries::new(0, EntriesEnd::More, 0).with_version(bound))?;
    answer.take(&Entries::new(0, EntriesEnd::More, 0).with_version(bound))?;
    let other = Entries::new(0, EntriesEnd::More, 0).with_version(bound - 1);
    assert_eq!(answer.offer(&other)?, Err(refused("version", 0)));
    Ok(())
}

#[test]
fn test_the_last_rule_comes_before_the_version_rule() -> TestResult {
    let mut answer = Answer::new(1);
    answer.take(&Entries::new(0, EntriesEnd::More, 0).with_version(2))?;
    let other = Entries::new(0, EntriesEnd::More, 0).with_version(4);
    assert_eq!(answer.flagged(&other, true)?, Err(refused("end", 0)));
    Ok(())
}

#[test]
fn test_a_runs_entry_is_a_first_key_at_or_after_the_entry_before() -> TestResult {
    let order = |index| refused("entry order", index);
    // A key no range of the run has first.
    let mut stranger = Entries::new(0, EntriesEnd::Ended, 0).with_through(3);
    stranger.push(b"\x1b\x09", b"id");
    assert_eq!(
        offered(&mut run_sequence(), &stranger, true)?,
        Err(order(0))
    );
    // A key of a range before the entry before's.
    let mut back = Entries::new(0, EntriesEnd::Ended, 0).with_through(3);
    back.push(RUN[2], b"id");
    back.push(RUN[0], b"id");
    assert_eq!(offered(&mut run_sequence(), &back, true)?, Err(order(1)));
    // A key of a range of the Scan outside the run.
    let mut sequence = run_sequence();
    let mut outside = Entries::new(0, EntriesEnd::Ended, 0).with_through(1);
    outside.push(RUN[2], b"id");
    assert_eq!(offered(&mut sequence, &outside, false)?, Err(order(0)));
    Ok(())
}

#[test]
fn test_a_batchs_entry_lies_within_its_range() -> TestResult {
    let order = |index| refused("entry order", index);
    let one = |key: &[u8]| {
        let mut batch = Entries::new(0, EntriesEnd::Ended, 0);
        batch.push(key, b"");
        batch
    };
    let bounded = || EntriesSequence::new([ScanRange::new(1, b"\x10b", b"\x10d")]);
    assert_eq!(
        offered(&mut bounded(), &one(b"\x10a"), true)?,
        Err(order(0))
    );
    assert_eq!(
        offered(&mut bounded(), &one(b"\x10d"), true)?,
        Err(order(0))
    );
    assert_eq!(offered(&mut bounded(), &one(b"\x10b"), true)?, Ok(()));
    assert_eq!(offered(&mut bounded(), &one(b"\x10c\xff"), true)?, Ok(()));
    let mut open = EntriesSequence::new([ScanRange::new(1, b"\x10b", b"")]);
    assert_eq!(offered(&mut open, &one(b"\x11"), true)?, Ok(()));
    Ok(())
}

/// A new range's first frame holding no entry leaves the range with no last key: the range before's last key binds
/// nothing after it.
#[test]
fn test_an_empty_first_batch_of_a_new_range_keeps_it_without_a_last_key() -> TestResult {
    let mut answer = after(2, EntriesEnd::Ended)?;
    answer.take(&Entries::new(1, EntriesEnd::More, 0))?;
    let batch = holding(Entries::new(1, EntriesEnd::Ended, 0), &[BELOW]);
    assert_eq!(answer.offer(&batch)?, Ok(()));
    Ok(())
}
