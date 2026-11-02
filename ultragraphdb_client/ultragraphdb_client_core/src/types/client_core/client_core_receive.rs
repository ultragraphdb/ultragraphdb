/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    AuthenticationStep, ClientAuthentication, Login, ReceivedFrame, TransportError,
};
use ultragraphdb_protocol_wire::{
    AuthenticateView, BeginView, BodyView, Command, CommitView, DescribeView, DropCommandView,
    ErrorCode, ErrorReplyView, FrameView, PrepareView, RegisterView, Welcome,
};

use crate::errors::client_error::{ClientError, ClientErrorEnum};
use crate::types::client_core::ClientCore;
use crate::types::committed::Committed;
use crate::types::link::Link;
use crate::types::pending_state::Expect;
use crate::types::reply::Reply;
use crate::types::session::Session;
use crate::types::session_state::SessionState;
use crate::types::statement::Statement;
use crate::types::stream_state::StreamState;
use crate::types::transaction::Transaction;

/// A refusal read from an Error frame.
pub(crate) fn refusal(view: &FrameView<'_>) -> Result<ClientError, ClientError> {
    let error = ErrorReplyView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(ClientError::refused(
        error.code(),
        error.detail(),
        error.message(),
        error.value().map(|value| value.to_term()),
    ))
}

impl<L: Login> ClientCore<L> {
    /// A frame the shell read, whole, at `now`, taken only at its place on the connection (`PROTOCOL.md` §2.5 steps
    /// 13 and 14), then routed to its stream and read in place through its view; a result's frame waits in its
    /// stream's inbox, its chunks checked when it is read. Stream 0 carries the Welcome, the Authenticate exchange and
    /// its renewals, and the server's closing Error. Refused, and the connection given up, when it breaks a
    /// validation rule ([`ClientError::protocol`]), when the handshake or an exchange fails
    /// ([`ClientError::transport`]), and when the server closes with an Error frame on stream 0
    /// ([`ClientError::closed`]); a request waiting on the connection then fails with
    /// [`ClientError::connection_lost`], the refusal its cause. A frame of a stream no longer open is late and is
    /// dropped.
    pub fn receive(&mut self, frame: ReceivedFrame, now: Instant) -> Result<(), ClientError> {
        self.last_progress = Some(now);
        if matches!(self.link, Link::Down) {
            return Ok(());
        }
        if let Err(rule) = frame.check_order() {
            return self.fatal(ClientError::protocol(rule));
        }
        if let Link::Handshake(handshake) = &self.link {
            return match handshake.receive(frame.bytes()) {
                Ok(welcome) => self.welcomed(welcome, now),
                Err(error) => self.fatal(ClientError::transport(error)),
            };
        }
        let stream = frame.stream();
        if stream == 0 {
            return self.connection_frame(&frame, now);
        }
        let Some(state) = self.streams.get_mut(&stream) else {
            return Ok(());
        };
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
            let view = match frame.view() {
                Ok(view) => view,
                Err(error) => return self.fatal(ClientError::protocol(error)),
            };
            match refusal(&view) {
                Ok(error) => {
                    if let Some(refused) = self.streams.get_mut(&stream) {
                        refused.failed = Some(error);
                    }
                    self.end_reads(stream);
                }
                Err(error) => return self.fatal(error),
            }
        }
        Ok(())
    }

    /// The server's Welcome: the connection is up, or its Authenticate exchange begins by a mechanism the login
    /// speaks; a login speaking none of the Welcome's gives the connection up.
    fn welcomed(&mut self, welcome: Welcome, now: Instant) -> Result<(), ClientError> {
        if !welcome.authenticates() {
            self.link = Link::Up(welcome);
            self.resend_commits(now);
            return Ok(());
        }
        match self.exchange(&welcome) {
            Ok(exchange) => {
                self.link = Link::Authenticating(welcome, exchange);
                Ok(())
            }
            Err(error) => self.fatal(error),
        }
    }

    /// A new exchange by a mechanism of `welcome`'s the login speaks, its first message queued on stream 0.
    pub(super) fn exchange(
        &mut self,
        welcome: &Welcome,
    ) -> Result<ClientAuthentication<L::Mechanism>, ClientError> {
        let mechanism = self
            .login
            .mechanism(welcome.mechanisms())
            .map_err(ClientError::transport)?
            .ok_or_else(|| {
                ClientError::transport(TransportError::authentication(
                    "the login speaks none of the server's mechanisms".to_string(),
                ))
            })?;
        let mut exchange = ClientAuthentication::new(mechanism);
        let first = exchange.start().map_err(ClientError::transport)?;
        let frame = self.connection_request(&first, welcome)?;
        self.outbox.push_back(frame);
        Ok(exchange)
    }

    /// A frame on stream 0: the server's next Authenticate of the exchange or its renewal, or its closing Error.
    fn connection_frame(&mut self, frame: &ReceivedFrame, now: Instant) -> Result<(), ClientError> {
        let view = match frame.view() {
            Ok(view) => view,
            Err(error) => return self.fatal(ClientError::protocol(error)),
        };
        match view.command() {
            Command::Error => {
                let closed = ErrorReplyView::from_frame(&view)
                    .map(|e| ClientError::closed(e.code(), e.message()));
                match closed {
                    Ok(error) => self.fatal(error),
                    Err(error) => self.fatal(ClientError::protocol(error)),
                }
            }
            Command::Authenticate => self.authenticate(&view, now),
            Command::Hello
            | Command::Welcome
            | Command::Register
            | Command::Begin
            | Command::Commit
            | Command::Abort
            | Command::Read
            | Command::Write
            | Command::Operation
            | Command::Prepare
            | Command::Execute
            | Command::Describe
            | Command::Result
            | Command::Credit
            | Command::Cancel
            | Command::Round
            | Command::Drop => Ok(()),
        }
    }

    /// The server's Authenticate: answered, or, at its final message, the connection acts for its principal until its
    /// expiry. A renewal must end as the same principal.
    fn authenticate(&mut self, view: &FrameView<'_>, now: Instant) -> Result<(), ClientError> {
        let welcome = match &self.link {
            Link::Authenticating(welcome, _) | Link::Up(welcome) => welcome.clone(),
            Link::Down | Link::Handshake(_) => return Ok(()),
        };
        let step = match (&mut self.link, self.renewal.as_mut()) {
            (Link::Authenticating(_, exchange), _) | (Link::Up(_), Some(exchange)) => {
                exchange.receive(view)
            }
            _ => {
                // An Authenticate with no exchange running is out of step, as one of the wrong step is.
                let step = AuthenticateView::from_frame(view).map_or(0, |a| a.step());
                return self.fatal(ClientError::protocol(ProtocolError::field_value(
                    Command::Authenticate.number(),
                    "step",
                    step.into(),
                )));
            }
        };
        match step {
            Ok(AuthenticationStep::Send(message)) => {
                let frame = self.connection_request(&message, &welcome)?;
                self.outbox.push_back(frame);
                Ok(())
            }
            Ok(AuthenticationStep::Done { principal, expires }) => {
                if self.renewal.take().is_some() && self.principal != Some(principal) {
                    return self.fatal(ClientError::transport(TransportError::authentication(
                        "renewed as another principal".to_string(),
                    )));
                }
                self.principal = Some(principal);
                self.schedule_renewal(expires, now);
                if matches!(self.link, Link::Authenticating(..)) {
                    self.link = Link::Up(welcome);
                    self.resend_commits(now);
                }
                Ok(())
            }
            Err(error) => self.fatal(ClientError::transport(error)),
        }
    }

    /// Gives the connection up for `error`, which the caller gets; requests waiting fail as lost, `error` their cause.
    fn fatal(&mut self, error: ClientError) -> Result<(), ClientError> {
        self.lose(&ClientError::connection_lost(Some(error.clone())));
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
        let view = match frame.view() {
            Ok(view) => view,
            Err(error) => return self.fatal(ClientError::protocol(error)),
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
            let _ended: Option<StreamState> = self.streams.remove(&stream);
            self.end_reads(stream);
        }
        if let Expect::Commit { session, request } | Expect::Drop { session, request } = expect {
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
    /// attempts left; a retry refused as evicted has an unknown outcome and is not retried, and an evicted session is
    /// forgotten; refused otherwise, the session keeps its request number.
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
        let code = match &outcome {
            Err(ClientError(ClientErrorEnum::Refused { code, .. })) => Some(*code),
            _ => None,
        };
        if code == Some(ErrorCode::SessionBusy) {
            pending.tries += 1;
            if pending.tries < attempts {
                pending.resend_at = Some(now + self.config.retry_delay());
                return;
            }
            pending.outcome = Some(Err(ClientError::retries_exhausted(
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
        if code == Some(ErrorCode::SessionEvicted) {
            pending.outcome = Some(if pending.retried {
                Err(ClientError::outcome_unknown(session, request))
            } else {
                outcome
            });
            let _forgotten: Option<SessionState> = self.sessions.remove(&session);
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
    /// is [`ClientError::reply_mismatch`], an answer of the request, and a reply breaking a rule is the caller's error.
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
            Expect::Drop { .. } => Command::Drop,
            Expect::Prepare => Command::Prepare,
            Expect::Describe => Command::Describe,
        };
        if command != wanted {
            return Ok(Err(ClientError::reply_mismatch(
                "command",
                wanted.number().into(),
                command.number().into(),
            )));
        }
        Ok(match expect {
            Expect::Register => {
                let number = RegisterView::from_frame(view)
                    .map_err(ClientError::protocol)?
                    .session();
                let _registered: &mut SessionState =
                    self.sessions.entry(number).or_insert(SessionState {
                        next_request: 1,
                        in_flight: false,
                        in_doubt: None,
                    });
                Ok(Reply::Registered(Session::new(number)))
            }
            Expect::Begin => {
                let begin = BeginView::from_frame(view).map_err(ClientError::protocol)?;
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
            Expect::Commit { session, request } => committed(view, session, request)?,
            Expect::Drop { session, request } => dropped(view, session, request)?,
            Expect::Prepare => {
                let prepare = PrepareView::from_frame(view).map_err(ClientError::protocol)?;
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
                    .map_err(ClientError::protocol)?;
                Ok(Reply::Described(describe))
            }
        })
    }
}

/// A Commit's reply `view` to commit `request` of `session`; one naming another commit is
/// [`ClientError::reply_mismatch`], and one breaking a rule the caller's error.
fn committed(
    view: &FrameView<'_>,
    session: u64,
    request: u64,
) -> Result<Result<Reply, ClientError>, ClientError> {
    let commit = CommitView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(if commit.session() != session {
        Err(ClientError::reply_mismatch(
            "session",
            session,
            commit.session(),
        ))
    } else if commit.request() != request {
        Err(ClientError::reply_mismatch(
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
    })
}

/// A Drop's reply `view` to drop `request` of `session`; one naming another drop is [`ClientError::reply_mismatch`],
/// and one breaking a rule the caller's error.
fn dropped(
    view: &FrameView<'_>,
    session: u64,
    request: u64,
) -> Result<Result<Reply, ClientError>, ClientError> {
    let drop = DropCommandView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(if drop.session() != session {
        Err(ClientError::reply_mismatch(
            "session",
            session,
            drop.session(),
        ))
    } else if drop.request() != request {
        Err(ClientError::reply_mismatch(
            "request",
            request,
            drop.request(),
        ))
    } else {
        Ok(Reply::Dropped(Committed::new(
            drop.commit_version(),
            request,
            Vec::new(),
        )))
    })
}
