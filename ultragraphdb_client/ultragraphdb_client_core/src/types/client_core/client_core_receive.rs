/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    AuthenticationStep, ClientAuthentication, Login, ReceivedFrame, TransportError,
};
use ultragraphdb_protocol_wire::{
    AuthenticateView, BeginView, BodyView, Command, CommitView, DescribeView, DropCommandView,
    ErrorCode, ErrorReplyView, FrameView, PrepareView, RefusedRequest, RegisterView, Welcome,
};

use crate::errors::client_error::{ClientError, ClientErrorEnum};
use crate::types::client_core::ClientCore;
use crate::types::committed::Committed;
use crate::types::link::Link;
use crate::types::pending_state::{Expect, PendingState};
use crate::types::reply::Reply;
use crate::types::session::Session;
use crate::types::session_state::SessionState;
use crate::types::statement::Statement;
use crate::types::stream_state::StreamState;
use crate::types::transaction::Transaction;

/// Where a frame the connection took belongs (`PROTOCOL.md` §2.5, step 3).
enum Place {
    /// Stream 0: a Welcome, an Authenticate or the server's closing Error.
    Connection,
    /// The reply of the request waiting on the stream, or its refusal.
    Reply(u64),
    /// A read's Result frame or its refusal.
    Read,
    /// The refusal of a request with no reply, on a transaction's stream.
    Refusal,
    /// A frame of a stream this side has ended: checked and dropped.
    Late,
}

/// A refusal of a request of the kind `request` names, read from an Error frame.
pub(crate) fn refusal(
    view: &FrameView<'_>,
    request: RefusedRequest,
) -> Result<ClientError, ClientError> {
    let error = ErrorReplyView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(ClientError::refused(
        error.code(),
        error.detail(),
        error.message(),
        error.value().map(|value| value.to_term()),
    )
    .of_request(request))
}

impl<L: Login> ClientCore<L> {
    /// A frame the shell read, whole, at `now`, checked in `PROTOCOL.md` §2.5's order, the framer having checked its
    /// header: its command at its place (step 3), its chunks, its sequence and its protocol (steps 9 to 14), then routed to
    /// its stream and read in place through its view; a result's frame waits in its stream's inbox, its chunks checked when
    /// it is read. Stream 0 carries the Welcome, the Authenticate exchange and its renewals, and the server's closing Error.
    /// Refused, and the connection given up, when it breaks a validation rule ([`ClientError::protocol`]), a command this
    /// side does not serve among them, a frame on a stream at or above the next it will open or by a command its request's
    /// "Answered by" does not list, when the handshake or an exchange fails ([`ClientError::transport`]), and when the
    /// server closes with an Error frame on stream 0 ([`ClientError::closed`]); a request waiting on the connection then
    /// fails with [`ClientError::connection_lost`], the refusal its cause. A Result past its stream's credit is refused as
    /// [`ProtocolError::past_credit`], which gives the connection up too (`PROTOCOL.md` §11.3). A frame of a stream this
    /// side has ended is late, whatever its command: it is checked and dropped.
    pub fn receive(&mut self, frame: ReceivedFrame, now: Instant) -> Result<(), ClientError> {
        self.last_progress = Some(now);
        if matches!(self.link, Link::Down) {
            return Ok(());
        }
        let place = match self.place(&frame) {
            Ok(place) => place,
            Err(rule) => return self.fatal(ClientError::protocol(rule)),
        };
        let stream = frame.stream();
        match place {
            Place::Read => {
                if let Err(rule) = frame.check_order() {
                    return self.fatal(ClientError::protocol(rule));
                }
                if let Some(state) = self.streams.get_mut(&stream) {
                    if frame.command() == Command::Result.number() {
                        let Some(left) = state.credit.checked_sub(1) else {
                            return self
                                .fatal(ClientError::protocol(ProtocolError::past_credit(stream)));
                        };
                        state.credit = left;
                    }
                    state.progress = Some(now);
                    state.inbox.push_back(frame);
                }
                Ok(())
            }
            Place::Connection => self.connection_frame(&frame, now),
            Place::Reply(id) => self.answer(id, stream, &frame, now),
            Place::Refusal => self.stream_refused(stream, &frame),
            Place::Late => match frame.view() {
                Ok(_late) => Ok(()),
                Err(rule) => self.fatal(ClientError::protocol(rule)),
            },
        }
    }

    /// Where `frame` belongs, or [`ProtocolError::command_not_served`] for a frame this side does not serve there: on
    /// stream 0, a Welcome before the handshake's end, then an Authenticate, or an Error; on another stream, a stream below
    /// the next this side will open, by a command its request's "Answered by" lists, or an Error.
    fn place(&self, frame: &ReceivedFrame) -> Result<Place, ProtocolError> {
        let stream = frame.stream();
        let not_served = ProtocolError::command_not_served(frame.command());
        let command = Command::from_number(frame.command())?;
        if stream == 0 {
            let own = match &self.link {
                Link::Handshake(_) => Command::Welcome,
                Link::Down | Link::Authenticating(..) | Link::Up(_) => Command::Authenticate,
            };
            return (command == own || command == Command::Error)
                .then_some(Place::Connection)
                .ok_or(not_served);
        }
        if stream >= self.next_stream {
            return Err(not_served);
        }
        let Some(state) = self.streams.get(&stream) else {
            return Ok(Place::Late);
        };
        if state.failed.is_some() || state.ended() {
            return Ok(Place::Late);
        }
        if let Some(id) = state.pending {
            let wanted = self.pending.get(&id).map(|p| p.expect.answered_by());
            return (command == Command::Error || wanted == Some(command))
                .then_some(Place::Reply(id))
                .ok_or(not_served);
        }
        if state.results {
            return (command == Command::Error || command == Command::Result)
                .then_some(Place::Read)
                .ok_or(not_served);
        }
        (command == Command::Error)
            .then_some(Place::Refusal)
            .ok_or(not_served)
    }

    /// An Error on a transaction's stream with no request waiting: the refusal of a request with no reply, which ends the
    /// transaction and its reads and is the answer of its next request.
    fn stream_refused(&mut self, stream: u64, frame: &ReceivedFrame) -> Result<(), ClientError> {
        let view = match frame.view() {
            Ok(view) => view,
            Err(error) => return self.fatal(ClientError::protocol(error)),
        };
        match refusal(&view, RefusedRequest::Other) {
            Ok(error) => {
                if let Some(refused) = self.streams.get_mut(&stream) {
                    refused.failed = Some(error);
                }
                self.end_reads(stream);
                Ok(())
            }
            Err(error) => self.fatal(error),
        }
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

    /// A frame on stream 0, its command one [`ClientCore::place`] found served there: the server's Welcome, its next
    /// Authenticate of the exchange or its renewal, or its closing Error.
    fn connection_frame(&mut self, frame: &ReceivedFrame, now: Instant) -> Result<(), ClientError> {
        let view = match frame.view() {
            Ok(view) => view,
            Err(error) => return self.fatal(ClientError::protocol(error)),
        };
        if let Link::Handshake(handshake) = &self.link {
            return match handshake.receive(frame.bytes()) {
                Ok(welcome) => self.welcomed(welcome, now),
                Err(error) => self.fatal(ClientError::transport(error)),
            };
        }
        if view.command() == Command::Authenticate {
            return self.authenticate(&view, now);
        }
        let closed =
            ErrorReplyView::from_frame(&view).map(|e| ClientError::closed(e.code(), e.message()));
        match closed {
            Ok(error) => self.fatal(error),
            Err(error) => self.fatal(ClientError::protocol(error)),
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

    /// The reply of request `id` waiting on `stream`, by a command [`ClientCore::place`] found it answered by.
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
            match refusal(&view, expect.refused_request()) {
                Ok(error) => Err(error),
                Err(error) => return self.fatal(error),
            }
        } else {
            match self.decode(expect, stream, &view, now) {
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
        let commit = self.pending.get(&id).and_then(PendingState::commit);
        if let Some(request) = commit {
            self.settle_commit(id, request, outcome, now);
        } else if expect == Expect::Register {
            self.settle_register(id, outcome, now);
        } else if let Some(pending) = self.pending.get_mut(&id) {
            pending.outcome = Some(outcome);
        }
        Ok(())
    }

    /// What the answer `outcome` of a Register settles: code 14 sends it again after the retry delay while it has a commit's
    /// attempts left, and then is its answer, as any other answer is (`PROTOCOL.md` §9.4, §11.3).
    fn settle_register(&mut self, id: u64, outcome: Result<Reply, ClientError>, now: Instant) {
        let attempts = self.config.commit_attempts();
        let delay = self.config.retry_delay();
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        pending.stream = None;
        pending.tries = pending.tries.saturating_add(1);
        if refused_code(&outcome) == Some(ErrorCode::SessionBusy) && pending.tries < attempts {
            pending.resend_at = Some(now.checked_add(delay).unwrap_or(now));
        } else {
            pending.outcome = Some(outcome);
        }
    }

    /// What the answer `outcome` of a commit, a drop or a one-frame write settles, by `PROTOCOL.md` §11.5. Through no session
    /// there is no resend, so code 17 is an unknown outcome. Through a session: a reply moves its counter and keeps its
    /// token; code 14 is sent again after the retry delay while it has attempts left, as is code 17 on a resend, and a first
    /// code 17 is in doubt at once, each in doubt once its attempts are spent; code 12 forgets the session and code 13 moves
    /// its counter above `detail`, each an unknown outcome on a resend; a resent Commit's code 18 did not commit, its other
    /// refusals leave it in doubt, as codes 21 and 23 leave any resend; any other refusal says the request wrote nothing, and
    /// its number stays.
    fn settle_commit(
        &mut self,
        id: u64,
        request: (u128, u64),
        outcome: Result<Reply, ClientError>,
        now: Instant,
    ) {
        let attempts = self.config.commit_attempts();
        let delay = self.config.retry_delay();
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        pending.stream = None;
        pending.ever_sent = true;
        let (session, number) = request;
        if session == 0 {
            let unavailable = refused_code(&outcome) == Some(ErrorCode::Unavailable);
            pending.outcome = Some(if unavailable {
                Err(ClientError::outcome_unknown(session, number))
            } else {
                outcome
            });
            return;
        }
        let state = self.sessions.get_mut(&session);
        match refused_code(&outcome) {
            Some(code)
                if code == ErrorCode::SessionBusy
                    || (code == ErrorCode::Unavailable && pending.retried) =>
            {
                pending.tries = pending.tries.saturating_add(1);
                if pending.tries < attempts {
                    pending.resend_at = Some(now.checked_add(delay).unwrap_or(now));
                } else {
                    pending.mark_in_doubt(request, state, outcome.err());
                }
            }
            Some(ErrorCode::Unavailable) => pending.mark_in_doubt(request, state, outcome.err()),
            Some(ErrorCode::SessionEvicted) => {
                pending.outcome = Some(if pending.retried {
                    Err(ClientError::outcome_unknown(session, number))
                } else {
                    outcome
                });
                let _forgotten: Option<SessionState> = self.sessions.remove(&session);
            }
            Some(ErrorCode::RequestOutOfOrder) => {
                if let (Some(state), Err(error)) = (state, &outcome) {
                    state.in_flight = false;
                    state.next_request = refused_detail(error).saturating_add(1);
                }
                pending.outcome = Some(if pending.retried {
                    Err(ClientError::outcome_unknown(session, number))
                } else {
                    outcome
                });
            }
            Some(code) if pending.retried && leaves_in_doubt(&pending.expect, code) => {
                pending.mark_in_doubt(request, state, outcome.err());
            }
            // A reply naming another commit says nothing of this one, which stays in doubt: only its stored reply or a
            // refusal saying it wrote nothing resolves it (`PROTOCOL.md` §11.5).
            None if is_mismatch(&outcome) => pending.mark_in_doubt(request, state, outcome.err()),
            Some(_) | None => {
                if let Some(state) = state {
                    state.in_flight = false;
                    if let Ok(Reply::Committed(answer) | Reply::Dropped(answer)) = &outcome {
                        state.next_request = state.next_request.max(number.saturating_add(1));
                        state.keep_token(answer.token());
                    }
                }
                pending.outcome = Some(outcome);
            }
        }
    }

    /// The reply `view` decoded for what its request expects, a command [`ClientCore::place`] found it answered by; a reply
    /// naming another commit is [`ClientError::reply_mismatch`], an answer of the request, and a reply breaking a rule is
    /// the caller's error.
    fn decode(
        &mut self,
        expect: Expect,
        stream: u64,
        view: &FrameView<'_>,
        now: Instant,
    ) -> Result<Result<Reply, ClientError>, ClientError> {
        Ok(match expect {
            Expect::Register => {
                let number = RegisterView::from_frame(view)
                    .map_err(ClientError::protocol)?
                    .session();
                let number = u128::from_be_bytes(number);
                let _registered: &mut SessionState = self
                    .sessions
                    .entry(number)
                    .or_insert_with(|| SessionState::new(1));
                Ok(Reply::Registered(Session::new(number)))
            }
            Expect::Begin => self.begun(stream, view, now)?,
            Expect::Commit { session, request } | Expect::Write { session, request } => {
                committed(view, session, request)?
            }
            Expect::Drop { session, request } => dropped(view, session, request)?,
            Expect::Prepare => {
                let prepare = PrepareView::from_frame(view).map_err(ClientError::protocol)?;
                Ok(Reply::Prepared(Statement::new(
                    prepare.statement(),
                    prepare.tenant(),
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

    /// A Begin's reply on `stream`: its transaction, its reads' deadline running from here, later than the server's, which
    /// runs from the Begin's arrival, by the Welcome's `transaction_ms` when it named none.
    fn begun(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        now: Instant,
    ) -> Result<Result<Reply, ClientError>, ClientError> {
        let begin = BeginView::from_frame(view).map_err(ClientError::protocol)?;
        let deadline_ms = match begin.deadline_ms() {
            0 => self.welcome().map_or(0, Welcome::transaction_ms),
            named => named,
        };
        if let Some(state) = self.streams.get_mut(&stream) {
            state.transaction = Some(begin.read_only());
            state.deadline = now.checked_add(Duration::from_millis(deadline_ms.into()));
        }
        Ok(Ok(Reply::Begun(Transaction::new(
            stream,
            self.generation,
            begin.tenant(),
            begin.read_version(),
            begin.level(),
            begin.read_only(),
        ))))
    }
}

/// The code of a refusal.
fn refused_code(outcome: &Result<Reply, ClientError>) -> Option<ErrorCode> {
    match outcome {
        Err(ClientError(ClientErrorEnum::Refused { code, .. })) => Some(*code),
        Ok(_) | Err(_) => None,
    }
}

/// Whether `outcome` is a reply naming another request than the one it answers.
fn is_mismatch(outcome: &Result<Reply, ClientError>) -> bool {
    matches!(
        outcome,
        Err(ClientError(ClientErrorEnum::ReplyMismatch { .. }))
    )
}

/// The `detail` of a refusal; 0 for any other error.
fn refused_detail(error: &ClientError) -> u64 {
    if let ClientErrorEnum::Refused { detail, .. } = &error.0 {
        *detail
    } else {
        0
    }
}

/// A Commit's reply `view` to commit `request` of `session`; one naming another commit is
/// [`ClientError::reply_mismatch`], and one breaking a rule the caller's error.
fn committed(
    view: &FrameView<'_>,
    session: u128,
    request: u64,
) -> Result<Result<Reply, ClientError>, ClientError> {
    let commit = CommitView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(if u128::from_be_bytes(commit.session()) != session {
        Err(ClientError::reply_mismatch(
            "session",
            session,
            u128::from_be_bytes(commit.session()),
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
        let nodes = (0..commit.result_count())
            .map(|i| commit.node(i).map(str::to_string))
            .collect();
        Ok(Reply::Committed(
            Committed::new(commit.commit_version(), request)
                .with_token(commit.token().cloned())
                .with_results(results, nodes),
        ))
    })
}

/// A Drop's reply `view` to drop `request` of `session`; one naming another drop is [`ClientError::reply_mismatch`],
/// and one breaking a rule the caller's error.
fn dropped(
    view: &FrameView<'_>,
    session: u128,
    request: u64,
) -> Result<Result<Reply, ClientError>, ClientError> {
    let drop = DropCommandView::from_frame(view).map_err(ClientError::protocol)?;
    Ok(if u128::from_be_bytes(drop.session()) != session {
        Err(ClientError::reply_mismatch(
            "session",
            session,
            u128::from_be_bytes(drop.session()),
        ))
    } else if drop.request() != request {
        Err(ClientError::reply_mismatch(
            "request",
            request,
            drop.request(),
        ))
    } else {
        Ok(Reply::Dropped(
            Committed::new(drop.commit_version(), request).with_token(drop.token().cloned()),
        ))
    })
}

/// Whether a resend's refusal `code` says nothing of the first send: any but code 18 for a Commit, and codes 21 and 23 for any
/// resend, which the frontend's own checks give before a partition's table decides (`PROTOCOL.md` §11.5).
fn leaves_in_doubt(expect: &Expect, code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::PermissionDenied | ErrorCode::CrossPartition
    ) || matches!(expect, Expect::Commit { .. }) && code != ErrorCode::NotCommitted
}
