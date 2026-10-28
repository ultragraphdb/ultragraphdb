/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BeginView, BodyView, Command, CommitView, DescribeView, ErrorCode, ErrorReplyView, FrameView,
    PrepareView, RegisterView,
};

use crate::errors::client_error::ClientError;
use crate::types::client_core::ClientCore;
use crate::types::committed::Committed;
use crate::types::link::Link;
use crate::types::pending_state::Expect;
use crate::types::reply::Reply;
use crate::types::session::Session;
use crate::types::session_state::SessionState;
use crate::types::statement::Statement;
use crate::types::transaction::Transaction;

/// A refusal read from an Error frame.
pub(crate) fn refusal(view: &FrameView<'_>) -> Result<ClientError, ClientError> {
    let error = ErrorReplyView::from_frame(view).map_err(ClientError::Protocol)?;
    Ok(ClientError::Refused(
        error.code(),
        error.detail(),
        error.message(),
        error.value().map(str::to_string),
    ))
}

impl ClientCore {
    /// A frame the shell read, whole, at `now`: routed to its stream, read in place through its view. Refused, and
    /// the connection given up, when it breaks a validation rule ([`ClientError::Protocol`]), when the handshake
    /// fails ([`ClientError::Transport`]), and when the server closes with an Error frame on stream 0
    /// ([`ClientError::Closed`]); a request waiting on the connection then fails with
    /// [`ClientError::ConnectionLost`]. A frame of a stream no longer open, or the same as the last of its stream, is
    /// a late or duplicated frame and is dropped.
    pub fn receive(&mut self, frame: ReceivedFrame, now: Instant) -> Result<(), ClientError> {
        self.last_progress = Some(now);
        match &self.link {
            Link::Down => return Ok(()),
            Link::Handshake(handshake) => {
                return match handshake.receive(frame.bytes()) {
                    Ok(welcome) => {
                        self.link = Link::Up(welcome);
                        self.resend_commits(now);
                        Ok(())
                    }
                    Err(error) => self.fatal(ClientError::Transport(error)),
                };
            }
            Link::Up(_) => {}
        }
        let stream = frame.stream();
        if stream == 0 {
            if frame.command() != Command::Error.number() {
                return Ok(());
            }
            let closed =
                FrameView::validate(frame.bytes(), self.config.cluster()).and_then(|view| {
                    ErrorReplyView::from_frame(&view)
                        .map(|e| ClientError::Closed(e.code(), e.message()))
                });
            return match closed {
                Ok(error) => self.fatal(error),
                Err(error) => self.fatal(ClientError::Protocol(error)),
            };
        }
        let checksum = frame
            .bytes()
            .get(..8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes);
        let Some(state) = self.streams.get_mut(&stream) else {
            return Ok(());
        };
        if state.last == checksum {
            return Ok(());
        }
        state.last = checksum;
        if let Some(id) = state.pending {
            return self.answer(id, stream, &frame, now);
        }
        if state.results {
            if frame.command() == Command::Result.number() {
                state.credit = state.credit.saturating_sub(1);
            }
            state.inbox.push_back(frame);
            return Ok(());
        }
        if frame.command() == Command::Error.number() {
            let view = match FrameView::validate(frame.bytes(), self.config.cluster()) {
                Ok(view) => view,
                Err(error) => return self.fatal(ClientError::Protocol(error)),
            };
            match refusal(&view) {
                Ok(error) => {
                    if let Some(state) = self.streams.get_mut(&stream) {
                        state.failed = Some(error);
                    }
                }
                Err(error) => return self.fatal(error),
            }
        }
        Ok(())
    }

    /// Gives the connection up for `error`, which the caller gets; requests waiting fail as lost.
    fn fatal(&mut self, error: ClientError) -> Result<(), ClientError> {
        self.lose(ClientError::ConnectionLost());
        Err(error)
    }

    /// The reply of request `id` waiting on `stream`.
    fn answer(
        &mut self,
        id: u64,
        stream: u64,
        frame: &ReceivedFrame,
        now: Instant,
    ) -> Result<(), ClientError> {
        let Some(expect) = self.pending.get(&id).map(|p| p.expect) else {
            return Ok(());
        };
        let view = match FrameView::validate(frame.bytes(), self.config.cluster()) {
            Ok(view) => view,
            Err(error) => return self.fatal(ClientError::Protocol(error)),
        };
        let outcome = if view.command() == Command::Error {
            match refusal(&view) {
                Ok(error) => Err(error),
                Err(error) => return self.fatal(error),
            }
        } else {
            match self.decode(expect, stream, &view) {
                Ok(outcome) => outcome,
                Err(error) => return self.fatal(error),
            }
        };
        let keeps_stream = matches!(outcome, Ok(Reply::Begun(_)));
        if let Some(state) = self.streams.get_mut(&stream) {
            state.pending = None;
        }
        if !keeps_stream {
            self.streams.remove(&stream);
        }
        if let Expect::Commit { session, request } = expect {
            return {
                self.settle_commit(id, session, request, outcome, now);
                Ok(())
            };
        }
        if let Some(pending) = self.pending.get_mut(&id) {
            pending.outcome = Some(outcome);
        }
        Ok(())
    }

    /// A commit's answer: committed, the session advances; busy, it is sent again after the retry delay while it has
    /// attempts left; refused, the session keeps its request number.
    fn settle_commit(
        &mut self,
        id: u64,
        session: u64,
        request: u64,
        outcome: Result<Reply, ClientError>,
        now: Instant,
    ) {
        let attempts = self.config.commit_attempts();
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        pending.stream = None;
        let busy = matches!(&outcome, Err(e) if matches!(e.0, crate::errors::client_error::ClientErrorEnum::Refused { code: ErrorCode::SessionBusy, .. }));
        if busy {
            pending.tries += 1;
            if pending.tries < attempts {
                pending.resend_at = Some(now + self.config.retry_delay());
                return;
            }
            pending.outcome = Some(Err(ClientError::RetriesExhausted(
                session,
                request,
                pending.tries,
            )));
            if let Some(state) = self.sessions.get_mut(&session) {
                state.in_flight = false;
                state.in_doubt = Some(request);
            }
            return;
        }
        if let Some(state) = self.sessions.get_mut(&session) {
            state.in_flight = false;
            if outcome.is_ok() {
                state.next_request = request + 1;
            }
        }
        pending.outcome = Some(outcome);
    }

    /// The reply `view` decoded for what its request expects; a reply of another command or naming another commit
    /// is [`ClientError::ReplyMismatch`], an answer of the request, and a reply breaking a rule is the caller's error.
    fn decode(
        &mut self,
        expect: Expect,
        stream: u64,
        view: &FrameView<'_>,
    ) -> Result<Result<Reply, ClientError>, ClientError> {
        let command = view.command();
        let wanted = match expect {
            Expect::Register => Command::Register,
            Expect::Begin => Command::Begin,
            Expect::Commit { .. } => Command::Commit,
            Expect::Prepare => Command::Prepare,
            Expect::Describe => Command::Describe,
        };
        if command != wanted {
            return Ok(Err(ClientError::ReplyMismatch(
                "command",
                wanted.number().into(),
                command.number().into(),
            )));
        }
        Ok(match expect {
            Expect::Register => {
                let number = RegisterView::from_frame(view)
                    .map_err(ClientError::Protocol)?
                    .session();
                self.sessions.entry(number).or_insert(SessionState {
                    next_request: 1,
                    in_flight: false,
                    in_doubt: None,
                });
                Ok(Reply::Registered(Session::new(number)))
            }
            Expect::Begin => {
                let begin = BeginView::from_frame(view).map_err(ClientError::Protocol)?;
                if let Some(state) = self.streams.get_mut(&stream) {
                    state.transaction = Some(begin.read_only());
                }
                Ok(Reply::Begun(Transaction::new(
                    stream,
                    self.generation,
                    begin.namespace(),
                    begin.read_version(),
                    begin.read_only(),
                )))
            }
            Expect::Commit { session, request } => {
                let commit = CommitView::from_frame(view).map_err(ClientError::Protocol)?;
                if commit.session() != session {
                    Err(ClientError::ReplyMismatch(
                        "session",
                        session,
                        commit.session(),
                    ))
                } else if commit.request() != request {
                    Err(ClientError::ReplyMismatch(
                        "request",
                        request,
                        commit.request(),
                    ))
                } else {
                    let results = (0..commit.result_count())
                        .filter_map(|i| commit.result(i))
                        .collect();
                    Ok(Reply::Committed(Committed::new(
                        commit.commit_version(),
                        request,
                        results,
                    )))
                }
            }
            Expect::Prepare => {
                let prepare = PrepareView::from_frame(view).map_err(ClientError::Protocol)?;
                Ok(Reply::Prepared(Statement::new(
                    prepare.statement(),
                    prepare.namespace(),
                    prepare.schema(),
                    prepare.parameter_types().to_vec(),
                    prepare.result_types().to_vec(),
                )))
            }
            Expect::Describe => {
                let describe = DescribeView::from_frame(view)
                    .and_then(|d| d.to_body())
                    .map_err(ClientError::Protocol)?;
                Ok(Reply::Described(describe))
            }
        })
    }
}
