/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go (NQuadBuffer). Changed for ultragraphdb: translated to Rust;
 * the channel is a bounded std channel whose receiver the constructor hands out.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod nquad_buffer_json;

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::metadata::HintType;

/// Batches N-Quads, sending each batch of `batch_size` to its channel; with a batch size of zero or less, everything
/// is sent in one batch on [`NQuadBuffer::flush`]. It also aggregates the type hints of the predicates.
///
/// As Go's channel, the channel holds 10 batches and a push waits while it is full. Go's `Ch()` hands out the
/// channel; here [`NQuadBuffer::new`] returns its receiver, which a consumer may move to another thread. Batches
/// for a dropped receiver are discarded, where Go's sender would block forever.
#[derive(Debug)]
pub struct NQuadBuffer {
    batch_size: i32,
    nquads: Vec<api::NQuad>,
    nq_ch: Option<SyncSender<Vec<api::NQuad>>>,
    pred_hints: HashMap<String, i32>,
}

impl NQuadBuffer {
    /// Dgraph's `NewNQuadBuffer`: a buffer sending batches of `batch_size`, and the receiver of its batches.
    pub fn new(batch_size: i32) -> (NQuadBuffer, Receiver<Vec<api::NQuad>>) {
        let (tx, rx) = sync_channel(10);
        let capacity = usize::try_from(batch_size).unwrap_or(0);
        let buf = NQuadBuffer {
            batch_size,
            nquads: Vec::with_capacity(capacity),
            nq_ch: Some(tx),
            pred_hints: HashMap::new(),
        };
        (buf, rx)
    }

    fn send(&self, batch: Vec<api::NQuad>) {
        let ch = self.nq_ch.as_ref().expect("NQuadBuffer used after flush");
        // A dropped receiver discards the batch.
        let _ = ch.send(batch);
    }

    /// Dgraph's `Push`: adds an N-Quad, sending the batch when it is full.
    ///
    /// # Panics
    ///
    /// When a full batch is sent after [`NQuadBuffer::flush`], as Go's send on the closed channel does.
    pub fn push(&mut self, nq: api::NQuad) {
        self.nquads.push(nq);
        if self.batch_size > 0 && self.nquads.len() >= self.batch_size as usize {
            let batch = std::mem::replace(
                &mut self.nquads,
                Vec::with_capacity(self.batch_size as usize),
            );
            self.send(batch);
        }
    }

    /// Dgraph's `Metadata`: the type hints aggregated so far.
    pub fn metadata(&self) -> pb::Metadata {
        pb::Metadata {
            pred_hints: self.pred_hints.clone(),
        }
    }

    /// Dgraph's `PushPredHint`: records a hint about the predicate's type; hints that disagree make it a list.
    pub fn push_pred_hint(&mut self, pred: &str, hint: HintType) {
        let mut hint = hint as i32;
        if let Some(&old) = self.pred_hints.get(pred)
            && hint != old
        {
            hint = HintType::List as i32;
        }
        self.pred_hints.insert(pred.to_string(), hint);
    }

    /// Dgraph's `Flush`: sends the N-Quads buffered and closes the channel. The buffer takes no more batches after.
    pub fn flush(&mut self) {
        if !self.nquads.is_empty() {
            let batch = std::mem::take(&mut self.nquads);
            self.send(batch);
        }
        self.nq_ch = None;
    }
}
