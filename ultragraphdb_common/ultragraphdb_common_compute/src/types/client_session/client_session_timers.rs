/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use super::ClientSession;
use crate::traits::authorizer::Authorizer;
use crate::types::queued::Queued;

impl<A: Authorizer + Clone> ClientSession<A> {
    /// The next time a timer of this connection is due.
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        let limits = self
            .transactions
            .values()
            .filter(|txn| txn.commit_out.is_none() && txn.ended.is_none())
            .filter_map(|txn| txn.limit);
        let one_frames = self.one_frames.values().filter_map(|one| one.limit);
        limits.chain(one_frames).min()
    }

    /// Whatever is due at `at`: a read-write transaction or a one-frame write past its limit.
    pub(super) fn timers(&mut self, at: Instant) {
        let past: Vec<u64> = self
            .transactions
            .iter()
            .filter(|(_, txn)| {
                txn.commit_out.is_none()
                    && txn.ended.is_none()
                    && txn.limit.is_some_and(|limit| limit <= at)
            })
            .map(|(stream, _)| *stream)
            .collect();
        for transaction in past {
            let refusal = self.past_limit(transaction);
            self.end_transaction(transaction, &refusal);
        }
        let unanswered: Vec<u64> = self
            .one_frames
            .iter()
            .filter(|(_, one)| one.limit.is_some_and(|limit| limit <= at))
            .map(|(stream, _)| *stream)
            .collect();
        for stream in unanswered {
            self.one_frame_past_limit(stream);
        }
    }

    /// Code 4, its message naming a read holding a write back with no credit (`PROTOCOL.md` §9.4).
    pub(super) fn past_limit(&self, transaction: u64) -> ErrorReply {
        let limit = self.core.limits().transaction_ms();
        let holding = self.transactions.get(&transaction).and_then(|txn| {
            let write_behind = txn
                .queue
                .iter()
                .any(|queued| matches!(queued, Queued::Write { .. }));
            if !write_behind {
                return None;
            }
            txn.reads_out
                .iter()
                .copied()
                .find(|read| self.reads.get(read).is_some_and(|state| state.credit == 0))
        });
        let message = match holding {
            Some(read) => format!(
                "the transaction passed its limit of {limit} ms, the read on stream {read} holding a write back: it had no credit"
            ),
            None => format!("the transaction passed its limit of {limit} ms"),
        };
        ErrorReply::new(ErrorCode::PastLimit, message).with_detail(u64::from(limit))
    }
}
