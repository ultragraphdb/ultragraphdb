/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Outbound, TransportError};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Commit, ErrorCode, ErrorReply, FLAG_LAST, Header, ResultBatch,
};

const CLUSTER: u64 = 1;

fn result(stream: u64, tag: u32, last: bool) -> Vec<u8> {
    // The column count tags the frame, so tests tell frames apart.
    ResultBatch::new(tag)
        .frame(
            Header::new(CLUSTER, Command::Result)
                .with_stream(stream)
                .with_flags(if last { FLAG_LAST } else { 0 }),
        )
        .unwrap()
}

fn reply(stream: u64, last: bool) -> Vec<u8> {
    Commit::new(1, 1, 2, Vec::new())
        .frame(
            Header::new(CLUSTER, Command::Commit)
                .with_stream(stream)
                .with_flags(if last { FLAG_LAST } else { 0 }),
        )
        .unwrap()
}

fn drain(outbound: &mut Outbound) -> Vec<(u64, Vec<u8>)> {
    std::iter::from_fn(|| outbound.next_frame()).collect()
}

#[test]
fn test_streams_open_within_their_limit() {
    let mut outbound = Outbound::new(2, 0);
    assert_eq!(outbound.open(0).unwrap_err(), TransportError::StreamZero());
    outbound.open(1).unwrap();
    assert_eq!(outbound.open(1).unwrap_err(), TransportError::StreamOpen(1));
    outbound.open(7).unwrap();
    assert_eq!(
        outbound.open(8).unwrap_err(),
        TransportError::StreamLimit(2)
    );
    assert_eq!(outbound.open_streams(), 2);
    assert!(outbound.is_open(7));
    assert!(!outbound.is_open(8));
    // A stream closes once its last frame has left, making room.
    outbound.push(1, reply(1, true)).unwrap();
    assert_eq!(drain(&mut outbound), vec![(1, reply(1, true))]);
    assert!(!outbound.is_open(1));
    outbound.open(8).unwrap();
}

#[test]
fn test_a_frame_for_a_stream_not_open_is_refused() {
    let mut outbound = Outbound::new(4, 0);
    assert_eq!(
        outbound.push(3, reply(3, true)).unwrap_err(),
        TransportError::UnknownStream(3)
    );
    outbound.open(3).unwrap();
    assert_eq!(
        outbound.push(3, vec![0; 10]).unwrap_err(),
        TransportError::Protocol(ProtocolError::ShortHeader(10))
    );
    assert_eq!(outbound.queued(3), 0);
}

#[test]
fn test_result_frames_leave_only_while_their_stream_holds_credit() {
    let mut outbound = Outbound::new(4, 0);
    outbound.open(5).unwrap();
    for tag in 0..3 {
        outbound.push(5, result(5, tag, tag == 2)).unwrap();
    }
    assert_eq!(outbound.queued(5), 3);
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(outbound.next_frame(), None);
    outbound.grant(5, 2);
    assert_eq!(
        drain(&mut outbound),
        vec![(5, result(5, 0, false)), (5, result(5, 1, false))]
    );
    assert_eq!(outbound.credit(5), Some(0));
    assert_eq!(outbound.queued(5), 1);
    outbound.grant(5, 10);
    assert_eq!(drain(&mut outbound), vec![(5, result(5, 2, true))]);
    assert!(!outbound.is_open(5));
    assert_eq!(outbound.credit(5), None);
    assert_eq!(outbound.queued(5), 0);
}

#[test]
fn test_initial_credit_applies_to_each_stream_and_grants_saturate() {
    let mut outbound = Outbound::new(4, 1);
    outbound.open(1).unwrap();
    outbound.open(2).unwrap();
    assert_eq!(outbound.credit(1), Some(1));
    outbound.grant(2, u32::MAX);
    assert_eq!(outbound.credit(2), Some(u32::MAX));
    // A grant for a stream not open is ignored.
    outbound.grant(9, 3);
    assert!(!outbound.is_open(9));
}

#[test]
fn test_replies_need_no_credit_but_wait_behind_their_streams_results() {
    let mut outbound = Outbound::new(4, 0);
    outbound.open(1).unwrap();
    outbound.push(1, reply(1, false)).unwrap();
    assert_eq!(drain(&mut outbound), vec![(1, reply(1, false))]);
    outbound.push(1, result(1, 0, false)).unwrap();
    outbound.push(1, reply(1, true)).unwrap();
    assert_eq!(
        outbound.next_frame(),
        None,
        "the reply keeps its place behind the result"
    );
    outbound.grant(1, 1);
    assert_eq!(
        drain(&mut outbound),
        vec![(1, result(1, 0, false)), (1, reply(1, true))]
    );
}

#[test]
fn test_streams_take_turns_in_order_of_their_ids() {
    let mut outbound = Outbound::new(4, 0);
    for stream in [2, 1, 3] {
        outbound.open(stream).unwrap();
    }
    for stream in [1, 2, 3] {
        outbound.push(stream, reply(stream, false)).unwrap();
        outbound.push(stream, reply(stream, true)).unwrap();
    }
    let order: Vec<u64> = drain(&mut outbound).into_iter().map(|(s, _)| s).collect();
    assert_eq!(order, vec![1, 2, 3, 1, 2, 3]);
    assert_eq!(outbound.open_streams(), 0);
}

#[test]
fn test_a_cancelled_stream_sends_nothing_more() {
    let mut outbound = Outbound::new(4, 10);
    outbound.open(1).unwrap();
    outbound.open(2).unwrap();
    for tag in 0..3 {
        outbound.push(1, result(1, tag, tag == 2)).unwrap();
        outbound.push(2, result(2, tag, tag == 2)).unwrap();
    }
    assert_eq!(outbound.next_frame(), Some((1, result(1, 0, false))));
    assert!(outbound.cancel(1));
    assert!(!outbound.cancel(1));
    assert!(!outbound.is_open(1));
    outbound.grant(1, 5);
    assert_eq!(
        outbound.push(1, result(1, 9, true)).unwrap_err(),
        TransportError::UnknownStream(1)
    );
    let rest = drain(&mut outbound);
    assert_eq!(
        rest.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![2, 2, 2]
    );
    let _ = ErrorReply::new(ErrorCode::Busy, "");
}
