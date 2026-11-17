/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Describe's answer continued across frames (`PROTOCOL.md` §8.3, *Continuation*): read as one body, its frames
//! spending the stream's credit as a read's do (§11.3), a frame breaking the first's command header or passing
//! `catalog_bytes_max` refused as `FieldValue` (`"describe"`), which closes the connection (§2.6).

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CATALOG_BYTES_MAX, Command, CreditView, DeclaredPredicate, Describe,
    DescribeEntry, EntryKind, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

use crate::utils::harness::{CLUSTER, Harness, config, described, server, welcome};

/// The line a source repeats: 19 bytes of ASCII.
const LINE: &str = "namespace gen5 { }\n";

/// An answer of tenant 0 read at version 42, laid out by hand: a label, a property key, a declared predicate, and an
/// installed source of `lines` lines, which decides how many frames it takes.
fn answer(lines: usize) -> Describe {
    Describe::new(0, 42)
        .with_labels(vec!["Person".to_string()])
        .with_property_keys(vec!["name".to_string()])
        .with_predicates(vec![DeclaredPredicate::new("http://ex.org/name")])
        .with_entries(vec![DescribeEntry::new(
            EntryKind::Source,
            "gen5",
            LINE.repeat(lines).into_bytes(),
        )])
}

/// The frames the wire's builder cuts `describe` into on stream 1 within `max_frame`: the server's answer, built
/// independently of the client.
fn frames_of(describe: &Describe, max_frame: usize) -> TestResult<Vec<Vec<u8>>> {
    Ok(describe.frames(
        Header::new(CLUSTER, Command::Describe).with_stream(1),
        max_frame,
    )?)
}

/// The frames of each Credit among `frames` the client sent on `stream`, in order.
fn credits(frames: &[Vec<u8>], stream: u64) -> TestResult<Vec<u32>> {
    let mut granted = Vec::new();
    for bytes in frames {
        let view = FrameView::validate_client(bytes, CLUSTER, MAX_FRAME_SIZE)?;
        if view.command() == Command::Credit && view.header().stream() == stream {
            granted.push(CreditView::from_frame(&view)?.frames());
        }
    }
    Ok(granted)
}

/// Whether `error` is the protocol's `FieldValue` naming `describe` on a Describe body (§8.3, *Continuation*), its value
/// what `value` accepts.
fn describe_refused(error: &ClientError, value: impl Fn(u64) -> bool) -> bool {
    matches!(
        &error.0,
        ClientErrorEnum::Protocol { error } if matches!(
            error.0,
            ProtocolErrorEnum::FieldValue { command: 12, field: "describe", value: v } if value(v)
        )
    )
}

/// wire-protocol, "A Describe continues across frames": an answer that takes two frames within the client's
/// `max_frame` reads as the one answer laid out by hand, within the Welcome's credit, so no Credit is sent.
#[test]
fn test_a_describe_continues_across_frames() -> TestResult {
    // 78,948 lines are 1,500,012 bytes: more than one frame of 1 MiB holds, less than two.
    let expected = answer(78_948);
    assert_eq!(frames_of(&expected, MAX_FRAME_SIZE)?.len(), 2, "two frames");
    let mut h = Harness::ready(server().with_describe(expected.clone()))?;
    let p = h.core.describe(0, h.now)?;
    let read = described(h.settle(p)?)?;
    assert_eq!(read, expected);
    assert!(h.core.is_ready(), "the connection goes on");
    assert_eq!(credits(h.server.received(), 1)?, Vec::<u32>::new());
    Ok(())
}

/// The catalog version an answer is drawn from reaches the caller, from an answer of one frame and from one of two, every
/// frame naming it; a tenant holding none is described at 0 (`PROTOCOL.md` §8.3).
#[test]
fn test_a_describe_answer_names_its_catalog_version() -> TestResult {
    // 78,948 lines take two frames, as above; 1 line takes one.
    for (lines, catalog) in [(1, 0), (1, 3), (78_948, 9)] {
        let expected = answer(lines).with_catalog(catalog);
        let frames = frames_of(&expected, MAX_FRAME_SIZE)?;
        for frame in &frames {
            let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
            assert_eq!(
                &view.header().command_header()[16..24],
                &catalog.to_le_bytes()
            );
        }
        let mut h = Harness::ready(server().with_describe(expected.clone()))?;
        let p = h.core.describe(0, h.now)?;
        let read = described(h.settle(p)?)?;
        assert_eq!(read.catalog(), expected.catalog(), "{lines} lines");
        assert_eq!(read, expected);
    }
    Ok(())
}

/// An answer of seven frames, past the Welcome's credit of 4, is read whole: the client grants its configured 2 frames
/// each time the count reaches 0 before the last frame, after the fourth and the sixth, and the server sends the rest
/// under them.
#[test]
fn test_a_describe_past_its_credit_is_granted_more_as_it_is_read() -> TestResult {
    // 340,000 lines are 6,460,000 bytes: six frames of 1 MiB and a seventh.
    let expected = answer(340_000);
    assert_eq!(
        frames_of(&expected, MAX_FRAME_SIZE)?.len(),
        7,
        "seven frames"
    );
    assert_eq!((welcome().initial_credit(), config().credit()), (4, 2));
    let mut h = Harness::ready(server().with_describe(expected.clone()))?;
    let p = h.core.describe(0, h.now)?;
    let read = described(h.settle(p)?)?;
    assert_eq!(read, expected);
    assert_eq!(credits(h.server.received(), 1)?, vec![2, 2]);
    Ok(())
}

/// Each frame of an answer spends the stream's credit, from the Welcome's 4: the frame that spends it, not the last,
/// grants the configured 2 more at once, after the fourth and the sixth; the eighth, the last, spends the count to 0
/// and grants nothing, since the stream ends with it. Only the last frame answers.
#[test]
fn test_each_describe_frame_spends_credit_and_a_spent_count_is_renewed() -> TestResult {
    // 1,480 lines are 28,120 bytes: eight frames of 4,096 bytes, each carrying at most 3,768 of them.
    let expected = answer(1_480);
    let frames = frames_of(&expected, 4_096)?;
    assert_eq!(frames.len(), 8, "eight frames");
    let mut h = Harness::ready(server())?;
    let p = h.core.describe(0, h.now)?;
    while h.core.transmit().is_some() {}
    let mut granted = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let frame = h.numbered(frame)?;
        h.deliver(&frame)??;
        let sent: Vec<Vec<u8>> = std::iter::from_fn(|| h.core.transmit()).collect();
        for frames in credits(&sent, 1)? {
            granted.push((index + 1, frames));
        }
        let answered = h.core.take_reply(p);
        assert_eq!(
            answered.is_some(),
            index + 1 == frames.len(),
            "frame {index}"
        );
        if let Some(reply) = answered {
            assert_eq!(described(reply)?, expected);
        }
    }
    assert_eq!(granted, vec![(4, 2), (6, 2)]);
    Ok(())
}

/// A frame of the answer whose command header is not the first's is refused as `FieldValue` (`"describe"`), and the
/// client closes the connection (§2.6): the Describe fails as lost, the refusal its cause.
#[test]
fn test_a_continuation_breaking_the_first_command_header_closes_the_connection() -> TestResult {
    let first = frames_of(&answer(1_480), 4_096)?;
    // The same answer read at version 43: its command header differs from the first's in `read_version` alone.
    let other = Describe::new(0, 43).with_entries(answer(1_480).entries().to_vec());
    let second = frames_of(&other, 4_096)?;
    let mut h = Harness::ready(server())?;
    let p = h.core.describe(0, h.now)?;
    while h.core.transmit().is_some() {}
    let frame = h.numbered(first.first().context("a first frame")?)?;
    h.deliver(&frame)??;
    let frame = h.numbered(second.get(1).context("a second frame")?)?;
    let error = h.deliver(&frame)?.err().context("refused")?;
    assert!(describe_refused(&error, |_| true), "{error:?}");
    assert!(!h.core.is_connected());
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(Some(error)))
    );
    Ok(())
}

/// wire-protocol, "A Describe past the catalog's bound is refused", the client's side: a stream whose sections pass
/// `catalog_bytes_max` together is refused as `FieldValue` (`"describe"`), naming the bytes taken, past the bound; the
/// client closes the connection and reads no further frame of it.
#[test]
fn test_a_describe_past_the_catalogs_bound_is_refused() -> TestResult {
    // Two MiB past the bound in the source alone: frames remain after the one that passes it.
    let lines = (CATALOG_BYTES_MAX + (2 << 20)) / LINE.len();
    let mut h = Harness::ready(server().with_describe(answer(lines)))?;
    let p = h.core.describe(0, h.now)?;
    let answered = h.settle(p)?.err().context("refused")?;
    let ClientErrorEnum::ConnectionLost { cause: Some(cause) } = &answered.0 else {
        return Err(TestError::new(format!("{answered:?}")));
    };
    let bound = CATALOG_BYTES_MAX as u64; // lossless: a `usize` is at most 64 bits
    assert!(describe_refused(cause, |held| held > bound), "{cause:?}");
    assert!(!h.core.is_connected(), "no further frame is read");
    Ok(())
}

/// An Error after the answer's first frame is the Describe's answer, as the frontend's code 6 for an answer past
/// `catalog_bytes_max` is (§8.3): the frames taken count for nothing, and the connection goes on.
#[test]
fn test_a_refusal_after_a_describes_first_frame_is_its_answer() -> TestResult {
    let frames = frames_of(&answer(1_480), 4_096)?;
    let mut h = Harness::ready(server())?;
    let p = h.core.describe(0, h.now)?;
    while h.core.transmit().is_some() {}
    let frame = h.numbered(frames.first().context("a first frame")?)?;
    h.deliver(&frame)??;
    let bound = CATALOG_BYTES_MAX as u64; // lossless: a `usize` is at most 64 bits
    let refusal = ErrorReply::new(ErrorCode::TooLarge, "past catalog_bytes_max")
        .with_detail(bound)
        .frame(
            Header::new(CLUSTER, Command::Error)
                .with_stream(1)
                .with_flags(FLAG_LAST),
        )?;
    let frame = h.numbered(&refusal)?;
    h.deliver(&frame)??;
    let error = h
        .core
        .take_reply(p)
        .context("an answer")?
        .err()
        .context("refused")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::TooLarge,
                detail,
                ..
            } if detail == bound
        ),
        "{error:?}"
    );
    assert!(h.core.is_ready());
    Ok(())
}

/// A connection lost between an answer's frames loses the Describe, with no cause for a plain close.
#[test]
fn test_a_connection_lost_between_a_describes_frames_loses_it() -> TestResult {
    let frames = frames_of(&answer(1_480), 4_096)?;
    let mut h = Harness::ready(server())?;
    let p = h.core.describe(0, h.now)?;
    while h.core.transmit().is_some() {}
    let frame = h.numbered(frames.first().context("a first frame")?)?;
    h.deliver(&frame)??;
    assert_eq!(h.core.take_reply(p), None, "the answer continues");
    h.core.disconnected(TransportError::closed(), h.now);
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(None))
    );
    Ok(())
}
