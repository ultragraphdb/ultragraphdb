/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Outbound, TransportError};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Commit, Entries, EntriesEnd, FLAG_LAST, FrameView, Header, HeaderView,
    MAX_FRAME_SIZE, ResultBatch,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 1;

fn result(stream: u64, tag: u32, last: bool) -> TestResult<Vec<u8>> {
    // The column count tags the frame, so tests tell frames apart.
    Ok(ResultBatch::new(tag).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 }),
    )?)
}

fn entries(stream: u64, range: u32, last: bool) -> TestResult<Vec<u8>> {
    // The range tags the frame, so tests tell frames apart.
    Ok(Entries::new(range, EntriesEnd::Ended, 0).frame(
        Header::new(CLUSTER, Command::Entries)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 }),
    )?)
}

fn reply(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    Ok(Commit::new(1u128.to_be_bytes(), 1, 2).frame(
        Header::new(CLUSTER, Command::Commit)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 }),
    )?)
}

/// `frame` with its sequence cleared, as it was pushed: the tests of scheduling compare frames apart from numbering.
fn unstamped((stream, mut frame): (u64, Vec<u8>)) -> TestResult<(u64, Vec<u8>)> {
    Header::stamp_sequence(&mut frame, 0)?;
    Ok((stream, frame))
}

fn next(outbound: &mut Outbound) -> TestResult<Option<(u64, Vec<u8>)>> {
    outbound.next_frame().map(unstamped).transpose()
}

fn drain(outbound: &mut Outbound) -> TestResult<Vec<(u64, Vec<u8>)>> {
    std::iter::from_fn(|| outbound.next_frame())
        .map(unstamped)
        .collect()
}

fn sequence(frame: &[u8]) -> TestResult<u128> {
    Ok(HeaderView::new(frame)?.sequence())
}

#[test]
fn test_a_stream_opened_with_its_credit_holds_it_and_its_limit() -> TestResult {
    let mut outbound = Outbound::new(1, 4);
    outbound.open_with_credit(1, 9)?;
    assert_eq!(outbound.credit(1), Some(9));
    assert_eq!(
        outbound
            .open_with_credit(1, 9)
            .err()
            .context("the refusal")?,
        TransportError::stream_open(1)
    );
    assert_eq!(
        outbound
            .open_with_credit(2, 9)
            .err()
            .context("the refusal")?,
        TransportError::stream_limit(1)
    );
    assert_eq!(outbound.credit(2), None);
    Ok(())
}

#[test]
fn test_streams_open_within_their_limit() -> TestResult {
    let mut outbound = Outbound::new(2, 0);
    assert_eq!(
        outbound.open(0).err().context("the refusal")?,
        TransportError::stream_zero()
    );
    outbound.open(1)?;
    assert_eq!(
        outbound.open(1).err().context("the refusal")?,
        TransportError::stream_open(1)
    );
    outbound.open(7)?;
    assert_eq!(
        outbound.open(8).err().context("the refusal")?,
        TransportError::stream_limit(2)
    );
    assert_eq!(outbound.open_streams(), 2);
    assert!(outbound.is_open(7));
    assert!(!outbound.is_open(8));
    // A stream closes once its last frame has left, making room.
    outbound.push(1, reply(1, true)?)?;
    assert_eq!(drain(&mut outbound)?, vec![(1, reply(1, true)?)]);
    assert!(!outbound.is_open(1));
    outbound.open(8)?;
    Ok(())
}

#[test]
fn test_a_frame_for_a_stream_not_open_is_refused() -> TestResult {
    let mut outbound = Outbound::new(4, 0);
    assert_eq!(
        outbound
            .push(3, reply(3, true)?)
            .err()
            .context("the refusal")?,
        TransportError::unknown_stream(3)
    );
    outbound.open(3)?;
    assert_eq!(
        outbound.push(3, vec![0; 10]).err().context("the refusal")?,
        TransportError::protocol(ProtocolError::short_header(10))
    );
    assert_eq!(outbound.queued(3), 0);
    Ok(())
}

/// `PROTOCOL.md` §11.3 and §13.14: on a role connection Entries frames spend credit as Result frames do.
#[test]
fn test_entries_frames_leave_only_while_their_stream_holds_credit() -> TestResult {
    let mut outbound = Outbound::new(4, 0);
    outbound.open(5)?;
    for range in 0..3 {
        outbound.push(5, entries(5, range, range == 2)?)?;
    }
    assert_eq!(next(&mut outbound)?, None);
    assert_eq!(outbound.credit(5), Some(0));
    outbound.grant(5, 2);
    assert_eq!(
        drain(&mut outbound)?,
        vec![(5, entries(5, 0, false)?), (5, entries(5, 1, false)?)]
    );
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(outbound.queued(5), 1);
    outbound.grant(5, 10);
    assert_eq!(drain(&mut outbound)?, vec![(5, entries(5, 2, true)?)]);
    assert!(!outbound.is_open(5));
    Ok(())
}

/// One stream's Result and Entries frames draw on one count.
#[test]
fn test_result_and_entries_frames_spend_one_credit() -> TestResult {
    let mut outbound = Outbound::new(4, 2);
    outbound.open(5)?;
    outbound.push(5, result(5, 0, false)?)?;
    outbound.push(5, entries(5, 0, false)?)?;
    outbound.push(5, entries(5, 1, false)?)?;
    assert_eq!(drain(&mut outbound)?.len(), 2);
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(outbound.queued(5), 1);
    Ok(())
}

#[test]
fn test_result_frames_leave_only_while_their_stream_holds_credit() -> TestResult {
    let mut outbound = Outbound::new(4, 0);
    outbound.open(5)?;
    for tag in 0..3 {
        outbound.push(5, result(5, tag, tag == 2)?)?;
    }
    assert_eq!(outbound.queued(5), 3);
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(next(&mut outbound)?, None);
    outbound.grant(5, 2);
    assert_eq!(
        drain(&mut outbound)?,
        vec![(5, result(5, 0, false)?), (5, result(5, 1, false)?)]
    );
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(outbound.queued(5), 1);
    outbound.grant(5, 10);
    assert_eq!(drain(&mut outbound)?, vec![(5, result(5, 2, true)?)]);
    assert!(!outbound.is_open(5));
    assert_eq!(outbound.credit(5), None);
    assert_eq!(outbound.queued(5), 0);
    Ok(())
}

#[test]
fn test_initial_credit_applies_to_each_stream_and_grants_saturate() -> TestResult {
    let mut outbound = Outbound::new(4, 1);
    outbound.open(1)?;
    outbound.open(2)?;
    assert_eq!(outbound.credit(1), Some(1));
    // A grant adds to what the stream holds: 1 + 3 (PROTOCOL.md section 11.3).
    outbound.grant(1, 3);
    assert_eq!(outbound.credit(1), Some(4));
    outbound.grant(2, u32::MAX);
    assert_eq!(outbound.credit(2), Some(u32::MAX));
    outbound.grant(2, 1);
    assert_eq!(outbound.credit(2), Some(u32::MAX), "saturating at 2^32 - 1");
    // A grant for a stream not open is ignored.
    outbound.grant(9, 3);
    assert!(!outbound.is_open(9));
    assert_eq!(outbound.credit(9), None);
    Ok(())
}

#[test]
fn test_transactions_and_reads_count_toward_the_stream_limit() -> TestResult {
    // Every open stream counts, whatever it carries (PROTOCOL.md section 11.2).
    let mut outbound = Outbound::new(3, 1);
    outbound.open_transaction(1)?;
    outbound.open_read(2, 1)?;
    outbound.open(3)?;
    assert_eq!(
        outbound.open_read(4, 1).err().context("the refusal")?,
        TransportError::stream_limit(3)
    );
    assert_eq!(
        outbound.open_transaction(5).err().context("the refusal")?,
        TransportError::stream_limit(3)
    );
    assert_eq!(outbound.open_streams(), 3);
    // A read's end makes room for one more.
    assert!(outbound.cancel(2));
    outbound.open_read(4, 1)?;
    assert_eq!(outbound.open_streams(), 3);
    // A limit of 0 opens nothing.
    let mut none = Outbound::new(0, 1);
    assert_eq!(
        none.open(1).err().context("the refusal")?,
        TransportError::stream_limit(0)
    );
    assert_eq!(none.open_streams(), 0);
    Ok(())
}

#[test]
fn test_replies_need_no_credit_but_wait_behind_their_streams_results() -> TestResult {
    let mut outbound = Outbound::new(4, 0);
    outbound.open(1)?;
    outbound.push(1, reply(1, false)?)?;
    assert_eq!(drain(&mut outbound)?, vec![(1, reply(1, false)?)]);
    outbound.push(1, result(1, 0, false)?)?;
    outbound.push(1, reply(1, true)?)?;
    assert_eq!(
        next(&mut outbound)?,
        None,
        "the reply keeps its place behind the result"
    );
    outbound.grant(1, 1);
    assert_eq!(
        drain(&mut outbound)?,
        vec![(1, result(1, 0, false)?), (1, reply(1, true)?)]
    );
    Ok(())
}

#[test]
fn test_streams_take_turns_in_order_of_their_ids() -> TestResult {
    let mut outbound = Outbound::new(4, 0);
    for stream in [2, 1, 3] {
        outbound.open(stream)?;
    }
    for stream in [1, 2, 3] {
        outbound.push(stream, reply(stream, false)?)?;
        outbound.push(stream, reply(stream, true)?)?;
    }
    let order: Vec<u64> = drain(&mut outbound)?.into_iter().map(|(s, _)| s).collect();
    assert_eq!(order, vec![1, 2, 3, 1, 2, 3]);
    assert_eq!(outbound.open_streams(), 0);
    Ok(())
}

#[test]
fn test_a_cancelled_stream_sends_nothing_more() -> TestResult {
    let mut outbound = Outbound::new(4, 10);
    outbound.open(1)?;
    outbound.open(2)?;
    for tag in 0..3 {
        outbound.push(1, result(1, tag, tag == 2)?)?;
        outbound.push(2, result(2, tag, tag == 2)?)?;
    }
    assert_eq!(next(&mut outbound)?, Some((1, result(1, 0, false)?)));
    assert!(outbound.cancel(1));
    assert!(!outbound.cancel(1));
    assert!(!outbound.is_open(1));
    outbound.grant(1, 5);
    assert_eq!(
        outbound
            .push(1, result(1, 9, true)?)
            .err()
            .context("the refusal")?,
        TransportError::unknown_stream(1)
    );
    let rest = drain(&mut outbound)?;
    assert_eq!(
        rest.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![2, 2, 2]
    );
    Ok(())
}

#[test]
fn test_every_frame_leaves_numbered_from_one_in_the_order_it_leaves() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    assert_eq!(outbound.sent(), 0);
    outbound.open(2)?;
    outbound.open(1)?;
    outbound.push(2, reply(2, true)?)?;
    outbound.push(1, reply(1, true)?)?;
    outbound.push_connection(reply(0, false)?)?;
    let sent = std::iter::from_fn(|| outbound.next_frame())
        .map(|(stream, frame)| -> TestResult<(u64, u128)> {
            // Each frame is whole and sound under its new sequence: the checksum follows it.
            let _view: FrameView<'_> = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)
                .context("a stamped frame validates")?;
            Ok((stream, sequence(&frame)?))
        })
        .collect::<TestResult<Vec<_>>>()?;
    assert_eq!(sent, vec![(0, 1), (1, 2), (2, 3)]);
    assert_eq!(outbound.sent(), 3);
    Ok(())
}

#[test]
fn test_the_connections_frames_go_first_in_the_order_pushed() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    outbound.push(1, result(1, 0, false)?)?;
    outbound.push_connection(reply(0, false)?)?;
    // A refusal of stream 9, never opened, leaves among the connection's frames.
    outbound.push_connection(reply(9, true)?)?;
    assert_eq!(
        drain(&mut outbound)?,
        vec![
            (0, reply(0, false)?),
            (9, reply(9, true)?),
            (1, result(1, 0, false)?)
        ]
    );
    assert!(!outbound.is_open(9));
    assert_eq!(
        outbound
            .push_connection(vec![0; 8])
            .err()
            .context("the refusal")?,
        TransportError::protocol(ProtocolError::short_header(8))
    );
    Ok(())
}

#[test]
fn test_a_read_opens_on_an_open_transaction_with_its_own_credit() -> TestResult {
    let mut outbound = Outbound::new(8, 2);
    assert_eq!(
        outbound.open_read(3, 1).err().context("the refusal")?,
        TransportError::unknown_stream(1),
        "no transaction on stream 1"
    );
    outbound.open(1)?;
    assert!(!outbound.is_transaction(1));
    assert_eq!(
        outbound.open_read(3, 1).err().context("the refusal")?,
        TransportError::unknown_stream(1),
        "stream 1 holds a request, not a transaction"
    );
    outbound.open_transaction(5)?;
    assert!(outbound.is_transaction(5));
    assert_eq!(
        outbound.open_transaction(5).err().context("the refusal")?,
        TransportError::stream_open(5)
    );
    outbound.open_read(3, 5)?;
    outbound.open_read(4, 5)?;
    assert!(!outbound.is_transaction(3));
    assert_eq!((outbound.credit(3), outbound.credit(4)), (Some(2), Some(2)));
    assert_eq!(
        outbound.open_read(4, 5).err().context("the refusal")?,
        TransportError::stream_open(4)
    );
    assert_eq!(
        outbound.open_read(0, 5).err().context("the refusal")?,
        TransportError::stream_zero()
    );
    Ok(())
}

#[test]
fn test_cancelling_a_read_keeps_its_transaction() -> TestResult {
    let mut outbound = Outbound::new(8, 4);
    outbound.open_transaction(1)?;
    outbound.open_read(2, 1)?;
    outbound.open_read(3, 1)?;
    outbound.push(2, result(2, 0, false)?)?;
    assert!(outbound.cancel(2));
    assert!(outbound.is_transaction(1));
    assert!(outbound.is_open(3));
    assert_eq!(outbound.queued(2), 0);
    Ok(())
}

#[test]
fn test_a_transactions_end_ends_its_reads() -> TestResult {
    // By its last frame leaving, the Commit's reply, which takes the first turn: the read's queued frames go with it.
    let mut outbound = Outbound::new(8, 4);
    outbound.open_transaction(1)?;
    outbound.open_read(2, 1)?;
    outbound.push(2, result(2, 0, false)?)?;
    outbound.push(2, result(2, 1, true)?)?;
    outbound.push(1, reply(1, true)?)?;
    assert_eq!(drain(&mut outbound)?, vec![(1, reply(1, true)?)]);
    assert!(!outbound.is_open(1));
    assert!(!outbound.is_open(2), "the read ended with its transaction");
    // By a cancel of the transaction: every read ends, a read of another transaction stays.
    let mut cancelled = Outbound::new(8, 4);
    cancelled.open_transaction(1)?;
    cancelled.open_transaction(5)?;
    cancelled.open_read(2, 1)?;
    cancelled.open_read(3, 1)?;
    cancelled.open_read(6, 5)?;
    assert!(cancelled.cancel(1));
    assert!(!cancelled.is_open(2) && !cancelled.is_open(3));
    assert!(cancelled.is_open(6) && cancelled.is_transaction(5));
    Ok(())
}

#[test]
fn test_the_connections_own_frames_are_taken_apart_from_the_streams() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    outbound.push(1, reply(1, true)?)?;
    outbound.push_connection(reply(0, false)?)?;
    let (stream, frame) = outbound
        .next_connection_frame()
        .context("the connection's frame")?;
    assert_eq!((stream, sequence(&frame)?), (0, 1));
    assert_eq!(unstamped((stream, frame))?, (0, reply(0, false)?));
    assert_eq!(
        outbound.next_connection_frame(),
        None,
        "a stream's reply is not the connection's"
    );
    assert!(outbound.is_open(1));
    let (reply_stream, reply_frame) = outbound.next_frame().context("the stream's reply")?;
    assert_eq!(
        (reply_stream, sequence(&reply_frame)?),
        (1, 2),
        "numbered after it"
    );
    Ok(())
}

#[test]
fn test_a_reply_spends_none_of_its_streams_credit() -> TestResult {
    // Credit meters Result frames alone (PROTOCOL.md section 11.3): a reply ahead of a result leaves it the credit.
    let mut outbound = Outbound::new(4, 1);
    outbound.open(1)?;
    outbound.push(1, reply(1, false)?)?;
    outbound.push(1, result(1, 0, true)?)?;
    assert_eq!(
        drain(&mut outbound)?,
        vec![(1, reply(1, false)?), (1, result(1, 0, true)?)]
    );
    Ok(())
}

/// Each Result or Entries frame not its stream's last reports the stream once, after it left.
#[test]
fn test_a_stream_is_reported_once_per_frame_pushed() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    assert!(outbound.take_ready().is_empty(), "nothing pushed yet");
    for range in 0..2 {
        outbound.push(1, entries(1, range, false)?)?;
        let _left = next(&mut outbound)?.context("the frame")?;
        assert_eq!(outbound.take_ready(), vec![1]);
        assert!(outbound.take_ready().is_empty(), "reported once");
    }
    outbound.open(3)?;
    outbound.push(3, result(3, 0, false)?)?;
    let _left = next(&mut outbound)?.context("the frame")?;
    assert_eq!(outbound.take_ready(), vec![3], "a Result frame as well");
    Ok(())
}

/// A frame still queued leaves no room, and neither does a stream with no credit, until a grant does.
#[test]
fn test_a_stream_is_not_reported_while_its_queue_holds_a_frame_or_its_credit_is_zero() -> TestResult
{
    let mut outbound = Outbound::new(4, 1);
    outbound.open(1)?;
    outbound.push(1, entries(1, 0, false)?)?;
    assert!(outbound.take_ready().is_empty(), "the frame has not left");
    let _left = next(&mut outbound)?.context("the frame")?;
    assert_eq!(outbound.credit(1), Some(0));
    assert!(outbound.take_ready().is_empty(), "no credit");
    outbound.grant(1, 1);
    assert_eq!(outbound.take_ready(), vec![1], "reported after the grant");
    outbound.push(1, entries(1, 1, false)?)?;
    outbound.push(1, entries(1, 2, false)?)?;
    let _left_too = next(&mut outbound)?.context("the frame")?;
    outbound.grant(1, 1);
    assert!(outbound.take_ready().is_empty(), "one frame still queued");
    Ok(())
}

/// A frame spending its stream's last credit left all the same: `take_left` says so, once, since no room follows it yet, and a
/// frame leaving with room needs only `take_ready`.
#[test]
fn test_a_frame_leaving_with_no_credit_is_reported_as_left() -> TestResult {
    let mut outbound = Outbound::new(4, 2);
    outbound.open(1)?;
    outbound.push(1, entries(1, 0, false)?)?;
    assert!(outbound.take_left().is_empty(), "the frame has not left");
    let _gone = next(&mut outbound)?.context("the frame")?;
    assert_eq!(outbound.take_ready(), vec![1], "room");
    assert!(outbound.take_left().is_empty(), "room says it left");
    outbound.push(1, entries(1, 1, false)?)?;
    let _gone_too = next(&mut outbound)?.context("the frame")?;
    assert!(outbound.take_ready().is_empty(), "no credit");
    assert_eq!(outbound.take_left(), vec![1]);
    assert!(outbound.take_left().is_empty(), "reported once");
    outbound.grant(1, 1);
    assert_eq!(outbound.take_ready(), vec![1], "the grant makes room");
    assert!(outbound.take_left().is_empty());
    Ok(())
}

/// A stream's last frame leaves nothing to report, and neither does a frame no credit meters.
#[test]
fn test_a_last_frame_is_not_reported_as_left() -> TestResult {
    let mut outbound = Outbound::new(4, 1);
    outbound.open(1)?;
    outbound.push(1, entries(1, 0, true)?)?;
    let _gone = next(&mut outbound)?.context("the frame")?;
    assert!(outbound.take_left().is_empty());
    Ok(())
}

/// A stream ended by its last frame or a cancel has no next frame to make room for.
#[test]
fn test_a_stream_ended_is_not_reported() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    outbound.push(1, entries(1, 0, true)?)?;
    let _left = next(&mut outbound)?.context("the frame")?;
    assert!(outbound.take_ready().is_empty(), "its last frame left");
    outbound.open(3)?;
    outbound.push(3, entries(3, 0, false)?)?;
    let _left_too = next(&mut outbound)?.context("the frame")?;
    assert!(outbound.cancel(3));
    assert!(outbound.take_ready().is_empty(), "cancelled");
    outbound.open(5)?;
    outbound.push(5, entries(5, 0, false)?)?;
    outbound.push(5, entries(5, 1, true)?)?;
    assert_eq!(drain(&mut outbound)?.len(), 2);
    assert!(
        outbound.take_ready().is_empty(),
        "its last frame followed the one that asked"
    );
    Ok(())
}

/// A stream carrying neither a Result nor an Entries frame is never reported, whatever its credit.
#[test]
fn test_a_stream_never_flagged_is_never_reported() -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    outbound.open(3)?;
    outbound.push(1, reply(1, false)?)?;
    let _left = next(&mut outbound)?.context("the reply")?;
    outbound.grant(3, 1);
    assert!(outbound.take_ready().is_empty());
    Ok(())
}

/// Stage 6.6: a Describe's and a Catalog's answer spend credit as Result frames do (`PROTOCOL.md` §11.3, §8.3, §13.7).
mod stage_6 {
    use ultragraphdb_protocol_transport::Outbound;
    use ultragraphdb_protocol_wire::{
        BodyBuilder, Catalog, Command, Describe, ErrorCode, ErrorReply, FLAG_LAST, Header,
    };
    use ultragraphdb_tool_bdd::TestResult;

    use super::{CLUSTER, drain, next};

    fn header(command: Command, stream: u64, last: bool) -> Header {
        Header::new(CLUSTER, command)
            .with_stream(stream)
            .with_flags(if last { FLAG_LAST } else { 0 })
    }

    /// Five frames of a Describe answer under an initial credit of 4: four leave, the fifth after a Credit of one.
    #[test]
    fn test_a_describes_frames_spend_credit() -> TestResult {
        let mut outbound = Outbound::new(4, 4);
        outbound.open(1)?;
        for index in 0..5u64 {
            let frame = Describe::new(0, 42).frame(header(Command::Describe, 1, index == 4))?;
            outbound.push(1, frame)?;
        }
        assert_eq!(drain(&mut outbound)?.len(), 4);
        assert_eq!(outbound.credit(1), Some(0));
        assert_eq!(outbound.queued(1), 1);
        assert_eq!(next(&mut outbound)?, None);
        outbound.grant(1, 1);
        assert_eq!(drain(&mut outbound)?.len(), 1);
        assert!(!outbound.is_open(1));
        Ok(())
    }

    /// A Catalog reply's frames spend credit too: one credit, two frames.
    #[test]
    fn test_a_catalogs_frames_spend_credit() -> TestResult {
        let mut outbound = Outbound::new(4, 1);
        outbound.open(3)?;
        for index in 0..2u64 {
            let frame = Catalog::reply(1, 7, 30, 2, vec![1, 2]).frame(header(
                Command::Catalog,
                3,
                index == 1,
            ))?;
            outbound.push(3, frame)?;
        }
        assert_eq!(drain(&mut outbound)?.len(), 1);
        assert_eq!(outbound.credit(3), Some(0));
        assert_eq!(outbound.queued(3), 1);
        Ok(())
    }

    /// A Describe frame spends its stream's one credit; an Error after it spends none and leaves with no credit left.
    #[test]
    fn test_an_error_on_a_describes_stream_spends_no_credit() -> TestResult {
        let mut outbound = Outbound::new(4, 1);
        outbound.open(1)?;
        outbound.push(
            1,
            Describe::new(0, 42).frame(header(Command::Describe, 1, false))?,
        )?;
        assert_eq!(drain(&mut outbound)?.len(), 1);
        assert_eq!(outbound.credit(1), Some(0));
        let refusal = ErrorReply::new(ErrorCode::Unavailable, "the metadata group")
            .frame(header(Command::Error, 1, true))?;
        outbound.push(1, refusal)?;
        assert_eq!(drain(&mut outbound)?.len(), 1);
        assert!(!outbound.is_open(1));
        Ok(())
    }
}
