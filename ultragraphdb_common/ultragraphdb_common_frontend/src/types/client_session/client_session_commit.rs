/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::spawn;
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CheckKind, CheckOutcome, Command, Commit, CommitView, ErrorCode,
    ErrorReply, FrameView, Header, MAX_FRAME_SIZE, Recall,
};

use super::{ClientSession, Recalling};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::client_transaction::ClientTransaction;
use crate::types::commit_hold::CommitHold;
use crate::types::link_event::LinkEvent;
use crate::types::queued::Queued;
use crate::types::recall_answer::RecallAnswer;
use crate::types::recall_decision::RecallDecision;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;
use crate::utils::recall_order::{recall_decision, second_check_answer};

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A Commit: of the transaction on `stream`, or a retry on a stream of its own.
    pub(super) async fn commit(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        let commit = CommitView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let session = u128::from_be_bytes(commit.session());
        let request = commit.request();
        let Some(txn) = self.transactions.get(&stream) else {
            return self.retried_commit(stream, session, request).await;
        };
        if txn.read_only {
            let refusal =
                ErrorReply::new(ErrorCode::Protocol, "a Commit in a read-only transaction");
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        if let Some(refusal) = txn.ended.clone() {
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        if !txn.declared.is_empty() {
            return Ok(self.declaring_commit(stream, session, request));
        }
        if session != 0
            && let Err(refusal) = self
                .core
                .system()
                .check(session.to_be_bytes(), self.principal, CheckKind::Marks)
                .await
        {
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        self.end_reads(stream);
        if let Some(open) = self.transactions.get_mut(&stream) {
            open.session_held = session;
            open.queue.push_back(Queued::Commit {
                frame: bytes.to_vec(),
                session,
            });
        }
        self.pump(stream);
        Ok(Answer::Open)
    }

    /// A Commit nothing bound, which reaches no partition: `commit_version` 0, the token zero, no results.
    pub(super) fn answer_empty_commit(&mut self, transaction: u64) {
        let Some(mut txn) = self.transactions.remove(&transaction) else {
            return;
        };
        let Some(Queued::Commit { frame, session }) = txn.queue.pop_front() else {
            return;
        };
        let request = FrameView::validate(&frame, self.cluster(), MAX_FRAME_SIZE)
            .ok()
            .and_then(|view| {
                CommitView::from_frame(&view)
                    .ok()
                    .map(|commit| commit.request())
            })
            .unwrap_or(0);
        self.release_session(&txn);
        self.refuse_reads_behind_commit(transaction);
        self.push_last(transaction, &Commit::new(session.to_be_bytes(), request, 0));
    }

    /// Refuses the reads `transaction` still holds, taken behind its Commit: a transaction not held open (`PROTOCOL.md` §7.1).
    fn refuse_reads_behind_commit(&mut self, transaction: u64) {
        let reads: Vec<u64> = self
            .reads
            .iter()
            .filter(|(_, read)| read.transaction == transaction)
            .map(|(read, _)| *read)
            .collect();
        let refusal = ErrorReply::new(
            ErrorCode::Protocol,
            format!("the transaction on stream {transaction} ended at its Commit"),
        );
        for read in reads {
            if let Some(ended) = self.reads.remove(&read) {
                if let Some((link, stream)) = &ended.route {
                    link.cancel(*stream);
                }
                self.push_error(read, &refusal);
            }
        }
    }

    /// Lets go of the session `txn`'s Commit marked in flight, if any.
    pub(super) fn release_session(&self, txn: &ClientTransaction) {
        if txn.session_held != 0 {
            self.core.system().ended(txn.session_held.to_be_bytes());
        }
    }

    /// A Commit's reply on the Bind stream, relayed; the transaction is over.
    pub(super) fn commit_reply(&mut self, transaction: u64, mut frame: Vec<u8>) {
        let Some(txn) = self.transactions.remove(&transaction) else {
            return;
        };
        self.release_session(&txn);
        self.refuse_reads_behind_commit(transaction);
        // A frame the framer took holds its header.
        drop(Header::stamp_stream(&mut frame, transaction));
        self.push(transaction, frame);
    }

    /// An Abort, or a Cancel on the transaction's stream: its reads end, its held writes are dropped, and its partition lets go,
    /// unless its Commit has reached the partition, which then decides it.
    pub(super) fn abort(&mut self, transaction: u64) {
        self.end_reads(transaction);
        let Some(txn) = self.transactions.remove(&transaction) else {
            return;
        };
        let behind: u64 = txn
            .queue
            .iter()
            .map(|queued| match queued {
                Queued::Write { behind, .. } => *behind,
                Queued::Read(_) | Queued::Commit { .. } => 0,
            })
            .sum();
        self.held = self.held.saturating_sub(behind);
        match &txn.bind {
            BindState::Binding { link, stream, .. } | BindState::Bound { link, stream, .. }
                if txn.commit_out.is_some() =>
            {
                // A Commit relayed meanwhile proceeds, its session in flight until its partition answers it; its outcome is
                // what a retry through the session learns (`PROTOCOL.md` §11.4, §11.5).
                if txn.session_held != 0 {
                    link.hold(
                        *stream,
                        CommitHold::new(self.core.system().clone(), txn.session_held),
                    );
                }
            }
            BindState::Binding { link, stream, .. } | BindState::Bound { link, stream, .. } => {
                self.release_session(&txn);
                // Cancelling a Bind stream aborts its transaction there (`PROTOCOL.md` §13.10).
                link.cancel(*stream);
            }
            BindState::None => self.release_session(&txn),
        }
    }

    /// A Commit on a stream of its own: refused through no session, and otherwise asked of every partition (`PROTOCOL.md` §11.5).
    async fn retried_commit(
        &mut self,
        stream: u64,
        session: u128,
        request: u64,
    ) -> Result<Answer, ErrorReply> {
        if session == 0 {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                "a Commit through no session on a new stream",
            ));
        }
        self.core
            .system()
            .check(session.to_be_bytes(), self.principal, CheckKind::Asks)
            .await?;
        let partitions = self.core.links().every_partition();
        let mut recalling = Recalling {
            session,
            request,
            unanswered: partitions.len(),
            ..Recalling::default()
        };
        for partition in &partitions {
            let Some(link) = self.core.links().partition(*partition) else {
                recalling.unanswered -= 1;
                recalling.answers.push(RecallAnswer::Unavailable);
                continue;
            };
            let header = link.header(Command::Recall, link.next_stream());
            let body = Recall::new(*partition, session.to_be_bytes(), request);
            match body.frame(header) {
                Ok(frame) => {
                    let sink = self.sink(RouteKey::Recall {
                        stream,
                        partition: *partition,
                    });
                    let _stream = link.open(self.counted(frame), sink);
                }
                Err(_) => {
                    recalling.unanswered -= 1;
                    recalling.answers.push(RecallAnswer::Unavailable);
                }
            }
        }
        debug_assert!(
            recalling.unanswered.saturating_add(recalling.answers.len()) == partitions.len(),
            "each partition is asked or answered"
        );
        let _none = self.recalls.insert(stream, recalling);
        self.recall_decides(stream);
        Ok(Answer::Open)
    }

    /// A partition's answer to a Recall.
    pub(super) fn recall_event(&mut self, stream: u64, partition: u32, event: LinkEvent) {
        let Some(recalling) = self.recalls.get_mut(&stream) else {
            return;
        };
        recalling.unanswered = recalling.unanswered.saturating_sub(1);
        let answer = match event {
            // A Recall is answered by a Commit's or a Drop's stored reply, the frame relayed as it came, or an Error; the link
            // closes on any other, so a Bind's, a Held or a Reference's answer stands as its close.
            LinkEvent::Closed
            | LinkEvent::Bound(_)
            | LinkEvent::Held
            | LinkEvent::Registered(_) => RecallAnswer::Unavailable,
            LinkEvent::Frame(frame) => {
                let _first = recalling.stored.get_or_insert(frame);
                RecallAnswer::Stored
            }
            LinkEvent::Refused(refusal) => {
                let code = refusal.code();
                if code == ErrorCode::SessionBusy {
                    RecallAnswer::Busy
                } else if code == ErrorCode::RequestOutOfOrder {
                    RecallAnswer::OutOfOrder(refusal.detail())
                } else if code == ErrorCode::NotCommitted {
                    RecallAnswer::NotCommitted
                } else {
                    tracing::debug!(partition, "a Recall was refused otherwise");
                    RecallAnswer::Unavailable
                }
            }
        };
        recalling.answers.push(answer);
        self.recall_decides(stream);
    }

    /// Answers a retried Commit once every partition has, in `PROTOCOL.md` §11.5's order: a stored reply, 14, 17, 13, then the
    /// second Check of kind 1 when every partition answered 18. A Recall marks nothing, so it ends with no Ended.
    fn recall_decides(&mut self, stream: u64) {
        let Some(recalling) = self
            .recalls
            .get(&stream)
            .filter(|recalling| recalling.unanswered == 0)
        else {
            return;
        };
        match recall_decision(&recalling.answers) {
            RecallDecision::SecondCheck => {
                let (session, principal) = (recalling.session, self.principal);
                let system = self.core.system().clone();
                let events = self.events_in.clone();
                spawn(async move {
                    let outcome = system
                        .check(session.to_be_bytes(), principal, CheckKind::Asks)
                        .await;
                    // A session gone has no one to tell.
                    drop(events.unbounded_send(SessionEvent::SecondCheck { stream, outcome }));
                })
                .detach();
            }
            RecallDecision::Stored => {
                let Some(ended) = self.recalls.remove(&stream) else {
                    return;
                };
                if let Some(mut stored) = ended.stored {
                    // A frame the framer took holds its header.
                    drop(Header::stamp_stream(&mut stored, stream));
                    self.push(stream, stored);
                }
            }
            RecallDecision::Refused(refusal) => {
                let _ended = self.recalls.remove(&stream);
                self.push_error(stream, &refusal);
            }
        }
    }

    /// Every partition answered 18: the second Check's `outcome` answers the client, 18 when it passed (`PROTOCOL.md` §11.5).
    pub(super) fn recall_session(&mut self, stream: u64, outcome: Result<(), ErrorReply>) {
        let Some(recalling) = self.recalls.remove(&stream) else {
            return;
        };
        let refusal = match outcome {
            Ok(()) => second_check_answer(CheckOutcome::Passed, recalling.request),
            Err(refusal) => refusal,
        };
        self.push_error(stream, &refusal);
    }

    /// A Commit of a transaction holding declarations: a catalog version built on the one current at its Begin, answered once its
    /// epoch is assigned (`PROTOCOL.md` §11.13).
    fn declaring_commit(&mut self, stream: u64, session: u128, request: u64) -> Answer {
        if session != 0 || request != 0 {
            let refusal = ErrorReply::new(
                ErrorCode::Refused,
                "a Commit of declarations names no session",
            );
            self.end_transaction(stream, &refusal);
            return Answer::Open;
        }
        let Some(txn) = self.transactions.get_mut(&stream) else {
            return Answer::Open;
        };
        let declared = std::mem::take(&mut txn.declared);
        let (tenant, built_on, principal) = (txn.tenant, txn.catalog, self.principal);
        txn.commit_out = Some(0);
        let system = self.core.system().clone();
        let events = self.events_in.clone();
        spawn(async move {
            let outcome = system
                .declare(tenant, built_on, principal, declared)
                .await
                .map(|(_, commit_version)| commit_version);
            // A session gone has no one to tell; the version stands.
            drop(events.unbounded_send(SessionEvent::Declared { stream, outcome }));
        })
        .detach();
        Answer::Open
    }

    /// The version a declaring Commit asked for took effect, or was refused.
    pub(super) fn declared(&mut self, stream: u64, outcome: Result<u64, ErrorReply>) {
        let Some(txn) = self.transactions.remove(&stream) else {
            return;
        };
        self.release_session(&txn);
        self.refuse_reads_behind_commit(stream);
        match outcome {
            Ok(version) => self.push_last(stream, &Commit::new([0; 16], 0, version)),
            Err(refusal) => self.push_error(stream, &refusal),
        }
    }
}
