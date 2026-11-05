/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod request_kind;

use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Authenticate, AuthenticateView, BodyBuilder, BodyView, Command, ComputeReadView, CreditView,
    ErrorCode, ErrorReply, ExecuteView, FLAG_LAST, FrameView, Header, HelloView, MAX_FRAME_FLOOR,
    QueryView, ReadView, Welcome,
};

use crate::errors::transport_error::{TransportError, TransportErrorEnum};
use crate::traits::authenticator::Authenticator;
use crate::traits::server_mechanism::ServerMechanism;
use crate::types::endpoint::Endpoint;
use crate::types::inbound::Inbound;
use crate::types::mechanism_step::MechanismStep;
use crate::types::outbound::Outbound;
use crate::types::received_frame::ReceivedFrame;
use crate::types::server_handshake::ServerHandshake;
use request_kind::RequestKind;

/// The refusal of every failed authentication, the same bytes whatever failed (`SECURITY.md`).
const AUTHENTICATION_FAILED: &str = "authentication failed";

/// The longest a connection takes from its Welcome to its final Authenticate (`PROTOCOL.md` §11.9).
pub const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(60);

/// The longest a connection sends no frame before the server closes it (`PROTOCOL.md` §11.9).
pub const IDLE_LIMIT: Duration = Duration::from_secs(600);

/// Where a connection stands: before its Hello, authenticating, or serving.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Hello,
    Authenticating,
    Serving,
    Closing,
}

/// A server's side of one connection, sans-I/O: the handshake, the Authenticate exchange and re-authentication, the
/// authentication timeout, the credential's expiry and the idle limit, and the stream rules of `PROTOCOL.md` §11.2,
/// over the [`Outbound`] that sends every frame it answers with. A frame the connection's rules take is handed to the
/// server's session as an [`Inbound::Request`] on its open stream; time is passed in.
#[derive(Debug)]
pub struct ServerConnection<A: Authenticator> {
    cluster: u64,
    server: Welcome,
    endpoint: Endpoint,
    authenticator: A,
    exchange: Option<(A::Mechanism, u32)>,
    phase: Phase,
    principal: Option<u64>,
    outbound: Outbound,
    welcomed: Option<Welcome>,
    authentication_timeout: Duration,
    idle_limit: Duration,
    opened: Instant,
    last_frame: Instant,
    expires: Option<Instant>,
}

impl<A: Authenticator> ServerConnection<A> {
    /// The connection of a node of `cluster` welcoming as `server` does, authenticating with `authenticator` when
    /// `server` lists a mechanism, accepted at `now`.
    pub fn new(cluster: u64, server: Welcome, authenticator: A, now: Instant) -> Self {
        let outbound = Outbound::new(server.max_streams(), server.initial_credit());
        Self {
            cluster,
            server,
            endpoint: Endpoint::Client,
            authenticator,
            exchange: None,
            phase: Phase::Hello,
            principal: None,
            outbound,
            welcomed: None,
            authentication_timeout: AUTHENTICATION_TIMEOUT,
            idle_limit: IDLE_LIMIT,
            opened: now,
            last_frame: now,
            expires: None,
        }
    }

    /// The connection with its authentication timeout and idle limit, [`AUTHENTICATION_TIMEOUT`] and [`IDLE_LIMIT`]
    /// by default.
    pub fn with_limits(mut self, authentication: Duration, idle: Duration) -> Self {
        self.authentication_timeout = authentication;
        self.idle_limit = idle;
        self
    }

    /// The connection taking the requests `endpoint` serves, [`Endpoint::Client`]'s by default.
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = endpoint;
        self
    }

    /// The client's next frame, read whole, at `now`: validated in place, then taken by the connection's rules.
    ///
    /// # Errors
    ///
    /// [`TransportError::frame_not_built`] when a frame the connection answers with, its Welcome, an Authenticate, a
    /// refusal or its closing Error, cannot be built, naming its command and why (a frame past
    /// [`ultragraphdb_protocol_wire::MAX_FRAME_SIZE`] names its size): nothing of it is queued, the connection is
    /// closing and has authenticated no one it had not before, and its caller closes it without a last frame.
    pub fn receive(
        &mut self,
        frame: &ReceivedFrame,
        now: Instant,
    ) -> Result<Inbound, TransportError> {
        let taken = self.take(frame, now);
        if taken.is_err() {
            self.phase = Phase::Closing;
            self.exchange = None;
        }
        taken
    }

    fn take(&mut self, frame: &ReceivedFrame, now: Instant) -> Result<Inbound, TransportError> {
        if self.phase == Phase::Closing {
            return Ok(Inbound::Closing { code: None });
        }
        self.last_frame = now;
        let view = match frame.view() {
            Ok(view) => view,
            Err(refusal) => return self.refuse_connection(&refusal),
        };
        if self.phase == Phase::Hello {
            return self.hello(&view, frame.bytes(), now);
        }
        let stream = view.header().stream();
        if stream == 0 {
            if view.command() == Command::Authenticate && self.server.authenticates() {
                return self.authenticate(&view, now);
            }
            return self.close(
                ErrorCode::Protocol,
                &TransportError::stream_zero().to_string(),
            );
        }
        if self.phase == Phase::Authenticating {
            return self.close(
                ErrorCode::Unauthenticated,
                "a request before the connection authenticated",
            );
        }
        self.request(stream, &view)
    }

    /// A frame the shell's framer refused, by a rule of its header: answered with an Error of code 1 on stream 0, its
    /// message the rule's text, and the connection closes.
    ///
    /// # Errors
    ///
    /// [`TransportError::frame_not_built`] when the Error cannot be built: the connection closes without it.
    pub fn refuse_connection(
        &mut self,
        refusal: &ProtocolError,
    ) -> Result<Inbound, TransportError> {
        self.close(ErrorCode::Protocol, &refusal.to_string())
    }

    /// The session's refusal of a request on `stream` by a body's rule or its own: `error` on the stream, flagged
    /// last, which ends it, and with a transaction's stream the transaction and its reads.
    ///
    /// # Errors
    ///
    /// [`TransportError::frame_not_built`] naming the Error command when `error` cannot be framed, such as a message
    /// past the largest frame: nothing is queued and the stream stays as it was, for the session to refuse otherwise.
    pub fn refuse(&mut self, stream: u64, error: &ErrorReply) -> Result<(), TransportError> {
        let header = Header::new(self.cluster, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        let frame = error
            .frame(header)
            .map_err(|e| TransportError::frame_not_built(Command::Error.number(), e))?;
        let queued = if self.outbound.is_open(stream) {
            self.outbound.push(stream, frame)
        } else {
            self.outbound.push_connection(frame)
        };
        debug_assert!(
            queued.is_ok(),
            "a built frame holds a header and its stream is open: {queued:?}"
        );
        Ok(())
    }

    /// The frames the connection sends, every one stamped with its sequence as it leaves; the session queues its
    /// replies on their streams here.
    pub fn outbound(&mut self) -> &mut Outbound {
        &mut self.outbound
    }

    /// The next frame to send, from [`Outbound::next_frame`]; once the connection closes, only its own frames, so
    /// its closing Error is the last frame it sends and no reply queued on a stream follows it.
    pub fn next_frame(&mut self) -> Option<(u64, Vec<u8>)> {
        if self.phase == Phase::Closing {
            return self.outbound.next_connection_frame();
        }
        self.outbound.next_frame()
    }

    /// The principal the connection acts for; `None` before it authenticated.
    pub fn principal(&self) -> Option<u64> {
        self.principal
    }

    /// The Welcome the connection was answered with; `None` before its Hello.
    pub fn welcome(&self) -> Option<&Welcome> {
        self.welcomed.as_ref()
    }

    /// Whether the connection closes once its queued frames are sent.
    pub fn is_closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    /// When the connection next needs [`ServerConnection::handle_timeout`]: the authentication timeout, the
    /// credential's expiry, the idle limit, whichever is first; `None` once it closes.
    pub fn next_deadline(&self) -> Option<Instant> {
        match self.phase {
            Phase::Closing => None,
            // A limit past what an `Instant` holds never passes, so it sets no deadline.
            Phase::Hello | Phase::Authenticating => {
                self.opened.checked_add(self.authentication_timeout)
            }
            Phase::Serving => {
                let idle = self.last_frame.checked_add(self.idle_limit);
                match (self.expires, idle) {
                    (Some(expires), Some(idle)) => Some(expires.min(idle)),
                    (expires, idle) => expires.or(idle),
                }
            }
        }
    }

    /// The deadlines passed at `now`: an authentication not complete in time closes the connection with code 20, an
    /// expired credential with code 22, an idle connection with no frame; `None` while none has passed.
    ///
    /// # Errors
    ///
    /// [`TransportError::frame_not_built`] when the closing Error cannot be built: the connection closes without it.
    pub fn handle_timeout(&mut self, now: Instant) -> Result<Option<Inbound>, TransportError> {
        match self.phase {
            Phase::Closing => Ok(None),
            Phase::Hello | Phase::Authenticating => {
                let due = self.opened.checked_add(self.authentication_timeout);
                if !due.is_some_and(|due| now >= due) {
                    return Ok(None);
                }
                self.close(
                    ErrorCode::Unauthenticated,
                    "the authentication did not complete in time",
                )
                .map(Some)
            }
            Phase::Serving => {
                if self.expires.is_some_and(|expires| now >= expires) {
                    return self
                        .close(ErrorCode::CredentialExpired, "the credential expired")
                        .map(Some);
                }
                let idle = self.last_frame.checked_add(self.idle_limit);
                Ok(idle.is_some_and(|idle| now >= idle).then(|| {
                    self.phase = Phase::Closing;
                    Inbound::Closing { code: None }
                }))
            }
        }
    }

    /// The Hello answered at `now` as [`ServerHandshake`] decides: its Welcome, from which the authentication timeout
    /// runs (`PROTOCOL.md` §11.9), or its refusal and the close.
    fn hello(
        &mut self,
        view: &FrameView<'_>,
        frame: &[u8],
        now: Instant,
    ) -> Result<Inbound, TransportError> {
        let (answer, outcome) = ServerHandshake::new(self.cluster, self.server.clone())
            .receive(frame)
            .into_parts();
        if let Some(answer) = answer {
            let queued = self.outbound.push_connection(answer);
            debug_assert!(queued.is_ok(), "a built frame holds a header: {queued:?}");
        }
        match outcome {
            Ok(welcome) => {
                let protocol = welcome.protocol();
                // The handshake took this frame as a Hello, so its view reads; the floor is what every client takes.
                let max_frame = HelloView::from_frame(view)
                    .map_or(MAX_FRAME_FLOOR as u32, |hello| hello.max_frame());
                if welcome.authenticates() {
                    self.phase = Phase::Authenticating;
                    self.opened = now;
                } else {
                    self.phase = Phase::Serving;
                    self.principal = Some(0);
                }
                self.welcomed = Some(welcome);
                Ok(Inbound::Welcomed {
                    protocol,
                    max_frame,
                })
            }
            Err(error) => {
                self.phase = Phase::Closing;
                let code = match error.0 {
                    TransportErrorEnum::Refused { code, .. } => code,
                    TransportErrorEnum::FrameNotBuilt { .. } => return Err(error),
                    TransportErrorEnum::Protocol { .. }
                    | TransportErrorEnum::Io { .. }
                    | TransportErrorEnum::Closed
                    | TransportErrorEnum::Truncated { .. }
                    | TransportErrorEnum::PoolExhausted { .. }
                    | TransportErrorEnum::StreamZero
                    | TransportErrorEnum::StreamOpen { .. }
                    | TransportErrorEnum::StreamLimit { .. }
                    | TransportErrorEnum::UnknownStream { .. }
                    | TransportErrorEnum::UnexpectedCommand { .. }
                    | TransportErrorEnum::Authentication { .. }
                    | TransportErrorEnum::Address { .. } => ErrorCode::Protocol,
                };
                Ok(Inbound::Closing { code: Some(code) })
            }
        }
    }

    /// One client message of an Authenticate exchange on stream 0.
    fn authenticate(
        &mut self,
        view: &FrameView<'_>,
        now: Instant,
    ) -> Result<Inbound, TransportError> {
        let message = match AuthenticateView::from_frame(view) {
            Ok(message) => message,
            Err(refusal) => return self.refuse_connection(&refusal),
        };
        let due = self.exchange.as_ref().map_or(0, |(_, due)| *due);
        if message.step() != due {
            return self.refuse_connection(&ProtocolError::field_value(
                Command::Authenticate.number(),
                "step",
                message.step().into(),
            ));
        }
        let mut mechanism = match self.exchange.take() {
            Some((mechanism, _)) => mechanism,
            None => match self.authenticator.start(message.mechanism()) {
                Some(mechanism) => mechanism,
                None => return self.close(ErrorCode::AuthenticationFailed, AUTHENTICATION_FAILED),
            },
        };
        let reply_step = due.saturating_add(1);
        match mechanism.step(message.message()) {
            Err(_) => self.close(ErrorCode::AuthenticationFailed, AUTHENTICATION_FAILED),
            Ok(MechanismStep::Continue(reply)) => {
                self.send_authenticate(&Authenticate::new(reply_step, "", reply))?;
                self.exchange = Some((mechanism, reply_step.saturating_add(1)));
                Ok(Inbound::Handled)
            }
            Ok(MechanismStep::Done(reply, credential)) => {
                let principal = credential.principal();
                if self.principal.is_some_and(|held| held != principal) {
                    return self.close(ErrorCode::AuthenticationFailed, AUTHENTICATION_FAILED);
                }
                // Sent before anything is recorded: a final message that cannot be built authenticates no one.
                self.send_authenticate(
                    &Authenticate::new(reply_step, "", reply)
                        .with_final(principal, credential.expires()),
                )?;
                self.principal = Some(principal);
                // A validity past what an `Instant` holds never expires.
                self.expires = credential
                    .valid_for()
                    .and_then(|valid_for| now.checked_add(valid_for));
                self.phase = Phase::Serving;
                Ok(Inbound::Authenticated { principal })
            }
        }
    }

    /// Queues `message` on stream 0; [`TransportError::frame_not_built`] naming Authenticate when it cannot be framed.
    fn send_authenticate(&mut self, message: &Authenticate) -> Result<(), TransportError> {
        let frame = message
            .frame(Header::new(self.cluster, Command::Authenticate))
            .map_err(|e| TransportError::frame_not_built(Command::Authenticate.number(), e))?;
        let queued = self.outbound.push_connection(frame);
        debug_assert!(queued.is_ok(), "a built frame holds a header: {queued:?}");
        Ok(())
    }

    /// Cancels `stream`, which the caller found open.
    fn cancel_open(&mut self, stream: u64) {
        let cancelled = self.outbound.cancel(stream);
        debug_assert!(
            cancelled,
            "stream {stream} was found open, so cancel closed it"
        );
    }

    /// A request on `stream`, by the stream rules of `PROTOCOL.md` §11.2.
    fn request(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Inbound, TransportError> {
        let command = view.command();
        let open = self.outbound.is_open(stream);
        let transaction = self.outbound.is_transaction(stream);
        let request = Inbound::Request { stream, command };
        let kind = if self.endpoint.only_bound(command) {
            RequestKind::InTransaction
        } else {
            RequestKind::of(command)
        };
        let endpoints_rule = !matches!(
            kind,
            RequestKind::Credit | RequestKind::Cancel | RequestKind::Unexpected
        );
        if endpoints_rule && !self.endpoint.serves(command) {
            return self.refuse_connection(&ProtocolError::command_not_served(command.number()));
        }
        match kind {
            RequestKind::Credit => self.credit(stream, view),
            RequestKind::Cancel => {
                if !open {
                    return Ok(Inbound::Handled);
                }
                self.cancel_open(stream);
                Ok(Inbound::Cancelled {
                    stream,
                    transaction,
                })
            }
            RequestKind::Begin
            | RequestKind::Open
            | RequestKind::ComputeRead
            | RequestKind::Read
                if open =>
            {
                self.stream_open(stream)
            }
            RequestKind::Begin => {
                let opened = self.outbound.open_transaction(stream);
                self.open(stream, opened, request)
            }
            RequestKind::Open => {
                let opened = self.outbound.open(stream);
                self.open(stream, opened, request)
            }
            RequestKind::ComputeRead => self.compute_read(stream, view, request),
            RequestKind::Read => self.read(stream, view, request),
            RequestKind::InTransaction if transaction => {
                if command == Command::Abort {
                    self.cancel_open(stream);
                }
                Ok(request)
            }
            RequestKind::InTransaction => self.refuse_with(
                stream,
                ErrorCode::Protocol,
                &format!("{command:?} on a stream that holds no transaction"),
            ),
            RequestKind::Commit if transaction => Ok(request),
            RequestKind::Commit if open => self.stream_open(stream),
            RequestKind::Commit => {
                let opened = self.outbound.open(stream);
                self.open(stream, opened, request)
            }
            RequestKind::Unexpected => self.close(
                ErrorCode::Protocol,
                &TransportError::unexpected_command(Command::Begin.number(), command.number())
                    .to_string(),
            ),
        }
    }

    /// A Credit on `stream`: its frames granted, or the frame refused on the stream by a body's rule.
    fn credit(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Inbound, TransportError> {
        match CreditView::from_frame(view) {
            Ok(credit) => self.outbound.grant(stream, credit.frames()),
            Err(rule) => return self.refuse_rule(stream, &rule),
        }
        Ok(Inbound::Handled)
    }

    /// A new request on `stream`, which is open: a connection error (`PROTOCOL.md` §2.6).
    fn stream_open(&mut self, stream: u64) -> Result<Inbound, TransportError> {
        self.close(
            ErrorCode::Protocol,
            &TransportError::stream_open(stream).to_string(),
        )
    }

    /// `request`, a ComputeRead on `stream`, once its stream opened with the credit it names (`PROTOCOL.md` §11.3); refused
    /// on the stream by a body's rule.
    fn compute_read(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        request: Inbound,
    ) -> Result<Inbound, TransportError> {
        match ComputeReadView::from_frame(view) {
            Ok(read) => {
                let opened = self.outbound.open_with_credit(stream, read.credit());
                self.open(stream, opened, request)
            }
            Err(rule) => self.refuse_rule(stream, &rule),
        }
    }

    /// `request`, a Read, an Execute or a Query on `stream`, once its stream opened for the transaction it names; refused
    /// on the stream by a body's rule.
    fn read(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        request: Inbound,
    ) -> Result<Inbound, TransportError> {
        let named = if view.command() == Command::Read {
            ReadView::from_frame(view).map(|read| read.transaction())
        } else if view.command() == Command::Query {
            QueryView::from_frame(view).map(|query| query.transaction())
        } else {
            ExecuteView::from_frame(view).map(|execute| execute.transaction())
        };
        match named {
            Ok(of) => {
                let opened = self.outbound.open_read(stream, of);
                self.open(stream, opened, request)
            }
            Err(rule) => self.refuse_rule(stream, &rule),
        }
    }

    /// `request` once its stream opened as `opened` says; a stream past the limit is refused on it with code 15, and a
    /// read of a transaction not open with code 1.
    fn open(
        &mut self,
        stream: u64,
        opened: Result<(), TransportError>,
        request: Inbound,
    ) -> Result<Inbound, TransportError> {
        match opened {
            Ok(()) => Ok(request),
            Err(error) => match error.0 {
                TransportErrorEnum::StreamLimit { limit } => {
                    let refusal = ErrorReply::new(ErrorCode::StreamLimit, error.to_string())
                        .with_detail(limit.into());
                    self.refuse(stream, &refusal)?;
                    Ok(Inbound::Refused {
                        stream,
                        code: ErrorCode::StreamLimit,
                    })
                }
                TransportErrorEnum::UnknownStream {
                    stream: transaction,
                } => self.refuse_with(
                    stream,
                    ErrorCode::Protocol,
                    &format!("no transaction is open on stream {transaction}"),
                ),
                TransportErrorEnum::Protocol { .. }
                | TransportErrorEnum::Io { .. }
                | TransportErrorEnum::Closed
                | TransportErrorEnum::Truncated { .. }
                | TransportErrorEnum::PoolExhausted { .. }
                | TransportErrorEnum::StreamZero
                | TransportErrorEnum::StreamOpen { .. }
                | TransportErrorEnum::Refused { .. }
                | TransportErrorEnum::UnexpectedCommand { .. }
                | TransportErrorEnum::Authentication { .. }
                | TransportErrorEnum::FrameNotBuilt { .. }
                | TransportErrorEnum::Address { .. } => {
                    self.close(ErrorCode::Protocol, &error.to_string())
                }
            },
        }
    }

    fn refuse_rule(
        &mut self,
        stream: u64,
        rule: &ProtocolError,
    ) -> Result<Inbound, TransportError> {
        self.refuse(stream, &ErrorReply::protocol(rule))?;
        Ok(Inbound::Refused {
            stream,
            code: ErrorCode::Protocol,
        })
    }

    fn refuse_with(
        &mut self,
        stream: u64,
        code: ErrorCode,
        message: &str,
    ) -> Result<Inbound, TransportError> {
        self.refuse(stream, &ErrorReply::new(code, message))?;
        Ok(Inbound::Refused { stream, code })
    }

    /// The connection's last frame, an Error of `code` on stream 0, queued; the connection closes, with no last frame
    /// and [`TransportError::frame_not_built`] naming the Error command when it cannot be built.
    fn close(&mut self, code: ErrorCode, message: &str) -> Result<Inbound, TransportError> {
        self.phase = Phase::Closing;
        self.exchange = None;
        let header = Header::new(self.cluster, Command::Error).with_flags(FLAG_LAST);
        let frame = ErrorReply::new(code, message)
            .frame(header)
            .map_err(|e| TransportError::frame_not_built(Command::Error.number(), e))?;
        let queued = self.outbound.push_connection(frame);
        debug_assert!(queued.is_ok(), "a built frame holds a header: {queued:?}");
        Ok(Inbound::Closing { code: Some(code) })
    }
}
