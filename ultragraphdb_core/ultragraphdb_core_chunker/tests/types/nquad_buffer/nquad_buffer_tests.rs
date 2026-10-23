/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph tests the buffer only through its parsers; these are written from chunker/json_parser.go.

use ultragraphdb_core_chunker::NQuadBuffer;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb::metadata::HintType;

fn nq(s: &str) -> api::NQuad {
    api::NQuad {
        subject: s.to_string(),
        ..Default::default()
    }
}

fn subjects(batch: &[api::NQuad]) -> Vec<&str> {
    batch.iter().map(|n| n.subject.as_str()).collect()
}

#[test]
fn a_positive_batch_size_sends_full_batches_and_the_rest_on_flush() {
    let (mut buf, rx) = NQuadBuffer::new(2);
    for s in ["a", "b", "c", "d", "e"] {
        buf.push(nq(s));
    }
    assert_eq!(subjects(&rx.recv().unwrap()), ["a", "b"]);
    assert_eq!(subjects(&rx.recv().unwrap()), ["c", "d"]);
    assert!(rx.try_recv().is_err());
    buf.flush();
    assert_eq!(subjects(&rx.recv().unwrap()), ["e"]);
    assert!(rx.recv().is_err(), "flush closes the channel");
}

#[test]
fn a_negative_batch_size_sends_everything_on_flush() {
    let (mut buf, rx) = NQuadBuffer::new(-1);
    for s in ["a", "b", "c"] {
        buf.push(nq(s));
    }
    assert!(rx.try_recv().is_err());
    buf.flush();
    assert_eq!(subjects(&rx.recv().unwrap()), ["a", "b", "c"]);
    assert!(rx.recv().is_err());
}

#[test]
fn flushing_nothing_sends_nothing() {
    let (mut buf, rx) = NQuadBuffer::new(10);
    buf.flush();
    assert!(rx.recv().is_err());
}

#[test]
fn a_dropped_receiver_does_not_stop_the_buffer() {
    let (mut buf, rx) = NQuadBuffer::new(1);
    drop(rx);
    buf.push(nq("a"));
    buf.flush();
}

#[test]
fn pred_hints_turn_into_list_on_disagreement() {
    let (mut buf, _rx) = NQuadBuffer::new(-1);
    buf.push_pred_hint("a", HintType::Single);
    buf.push_pred_hint("a", HintType::Single);
    buf.push_pred_hint("b", HintType::Single);
    buf.push_pred_hint("b", HintType::List);
    buf.push_pred_hint("b", HintType::Single);
    buf.push_pred_hint("c", HintType::List);
    let md = buf.metadata();
    assert_eq!(md.pred_hints.len(), 3);
    assert_eq!(md.pred_hints["a"], HintType::Single as i32);
    assert_eq!(md.pred_hints["b"], HintType::List as i32);
    assert_eq!(md.pred_hints["c"], HintType::List as i32);
}
