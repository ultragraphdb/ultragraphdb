/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Authenticate, AuthenticateView, BodyBuilder, BodyView, Command, CreditView, ErrorCode,
    ErrorReply, ExecuteView, FLAG_LAST, FrameView, Header, ReadView, Welcome,
};

use crate::errors::transport_error::{TransportError, TransportErrorEnum};
use crate::traits::authenticator::Authenticator;
use crate::traits::server_mechanism::ServerMechanism;
use crate::types::inbound::Inbound;
use crate::types::mechanism_step::MechanismStep;
use crate::types::outbound::Outbound;
use crate::types::received_frame::ReceivedFrame;
use crate::types::server_handshake::ServerHandshake;

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

    /// The client's next frame, read whole, at `now`: validated in place, then taken by the connection's rules.
    pub fn receive(&mut self, frame: &ReceivedFrame, now: Instant) -> Inbound {
        if self.phase == Phase::Closing {
            return Inbound::Closing { code: None };
        }
        self.last_frame = now;
        let view = match frame.view() {
            Ok(view) => view,
            Err(refusal) => return self.refuse_connection(&refusal),
        };
        if self.phase == Phase::Hello {
            return self.hello(frame.bytes(), now);
        }
        let stream = view.header().stream();
        if stream == 0 {
            return match view.command() {
                Command::Authenticate if self.server.authenticates() => {
                    self.authenticate(&view, now)
                }
                _ => self.close(
                    ErrorCode::Protocol,
                    &TransportError::StreamZero().to_string(),
                ),
            };
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
    pub fn refuse_connection(&mut self, refusal: &ProtocolError) -> Inbound {
        self.close(ErrorCode::Protocol, &refusal.to_string())
    }

    /// The session's refusal of a request on `stream` by a body's rule or its own: `error` on the stream, flagged
    /// last, which ends it, and with a transaction's stream the transaction and its reads.
    pub fn refuse(&mut self, stream: u64, error: &ErrorReply) {
        let header = Header::new(self.cluster, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        let Ok(frame) = error.frame(header) else {
            return;
        };
        if self.outbound.is_open(stream) {
            let _ = self.outbound.push(stream, frame);
        } else {
            let _ = self.outbound.push_connection(frame);
        }
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
            Phase::Hello | Phase::Authenticating => Some(self.opened + self.authentication_timeout),
            Phase::Serving => {
                let idle = self.last_frame + self.idle_limit;
                Some(self.expires.map_or(idle, |expires| expires.min(idle)))
            }
        }
    }

    /// The deadlines passed at `now`: an authentication not complete in time closes the connection with code 20, an
    /// expired credential with code 22, an idle connection with no frame; `None` while none has passed.
    pub fn handle_timeout(&mut self, now: Instant) -> Option<Inbound> {
        match self.phase {
            Phase::Closing => None,
            Phase::Hello | Phase::Authenticating => {
                (now >= self.opened + self.authentication_timeout).then(|| {
                    self.close(
                        ErrorCode::Unauthenticated,
                        "the authentication did not complete in time",
                    )
                })
            }
            Phase::Serving => {
                if self.expires.is_some_and(|expires| now >= expires) {
                    return Some(
                        self.close(ErrorCode::CredentialExpired, "the credential expired"),
                    );
                }
                (now >= self.last_frame + self.idle_limit).then(|| {
                    self.phase = Phase::Closing;
                    Inbound::Closing { code: None }
                })
            }
        }
    }

    /// The Hello answered at `now` as [`ServerHandshake`] decides: its Welcome, from which the authentication timeout
    /// runs (`PROTOCOL.md` §11.9), or its refusal and the close.
    fn hello(&mut self, frame: &[u8], now: Instant) -> Inbound {
        let (answer, outcome) = ServerHandshake::new(self.cluster, self.server.clone())
            .receive(frame)
            .into_parts();
        if let Some(answer) = answer {
            let _ = self.outbound.push_connection(answer);
        }
        match outcome {
            Ok(welcome) => {
                let protocol = welcome.protocol();
                if welcome.authenticates() {
                    self.phase = Phase::Authenticating;
                    self.opened = now;
                } else {
                    self.phase = Phase::Serving;
                    self.principal = Some(0);
                }
                self.welcomed = Some(welcome);
                Inbound::Welcomed { protocol }
            }
            Err(error) => {
                self.phase = Phase::Closing;
                let code = match error.0 {
                    TransportErrorEnum::Refused { code, .. } => code,
                    _ => ErrorCode::Protocol,
                };
                Inbound::Closing { code: Some(code) }
            }
        }
    }

    /// One client message of an Authenticate exchange on stream 0.
    fn authenticate(&mut self, view: &FrameView<'_>, now: Instant) -> Inbound {
        let message = match AuthenticateView::from_frame(view) {
            Ok(message) => message,
            Err(refusal) => return self.refuse_connection(&refusal),
        };
        let due = self.exchange.as_ref().map_or(0, |(_, due)| *due);
        if message.step() != due {
            return self.refuse_connection(&ProtocolError::FieldValue(
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
                self.send_authenticate(Authenticate::new(reply_step, "", reply));
                self.exchange = Some((mechanism, reply_step.saturating_add(1)));
                Inbound::Handled
            }
            Ok(MechanismStep::Done(reply, credential)) => {
                let principal = credential.principal();
                if self.principal.is_some_and(|held| held != principal) {
                    return self.close(ErrorCode::AuthenticationFailed, AUTHENTICATION_FAILED);
                }
                self.send_authenticate(
                    Authenticate::new(reply_step, "", reply)
                        .with_final(principal, credential.expires()),
                );
                self.principal = Some(principal);
                self.expires = credential.valid_for().map(|valid_for| now + valid_for);
                self.phase = Phase::Serving;
                Inbound::Authenticated { principal }
            }
        }
    }

    fn send_authenticate(&mut self, message: Authenticate) {
        if let Ok(frame) = message.frame(Header::new(self.cluster, Command::Authenticate)) {
            let _ = self.outbound.push_connection(frame);
        }
    }

    /// A request on `stream`, by the stream rules of `PROTOCOL.md` §11.2.
    fn request(&mut self, stream: u64, view: &FrameView<'_>) -> Inbound {
        let command = view.command();
        let open = self.outbound.is_open(stream);
        let transaction = self.outbound.is_transaction(stream);
        let request = Inbound::Request { stream, command };
        match command {
            Command::Credit => {
                match CreditView::from_frame(view) {
                    Ok(credit) => self.outbound.grant(stream, credit.frames()),
                    Err(rule) => return self.refuse_rule(stream, &rule),
                }
                Inbound::Handled
            }
            Command::Cancel => {
                if !open {
                    return Inbound::Handled;
                }
                self.outbound.cancel(stream);
                Inbound::Cancelled {
                    stream,
                    transaction,
                }
            }
            Command::Begin
            | Command::Register
            | Command::Prepare
            | Command::Describe
            | Command::Drop
            | Command::Read
            | Command::Execute
                if open =>
            {
                self.close(
                    ErrorCode::Protocol,
                    &TransportError::StreamOpen(stream).to_string(),
                )
            }
            Command::Begin => {
                let opened = self.outbound.open_transaction(stream);
                self.open(stream, opened, request)
            }
            Command::Register | Command::Prepare | Command::Describe | Command::Drop => {
                let opened = self.outbound.open(stream);
                self.open(stream, opened, request)
            }
            Command::Read | Command::Execute => {
                let named = if command == Command::Read {
                    ReadView::from_frame(view).map(|read| read.transaction())
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
            Command::Write | Command::Operation | Command::Abort if transaction => {
                if command == Command::Abort {
                    self.outbound.cancel(stream);
                }
                request
            }
            Command::Write | Command::Operation | Command::Abort => self.refuse_with(
                stream,
                ErrorCode::Protocol,
                "the stream holds no transaction",
            ),
            Command::Commit if transaction => request,
            Command::Commit if open => self.close(
                ErrorCode::Protocol,
                &TransportError::StreamOpen(stream).to_string(),
            ),
            Command::Commit => {
                let opened = self.outbound.open(stream);
                self.open(stream, opened, request)
            }
            other => self.close(
                ErrorCode::Protocol,
                &TransportError::UnexpectedCommand(Command::Begin.number(), other.number())
                    .to_string(),
            ),
        }
    }

    /// `request` once its stream opened as `opened` says; a stream past the limit is refused on it with code 15, and a
    /// read of a transaction not open with code 1.
    fn open(
        &mut self,
        stream: u64,
        opened: Result<(), TransportError>,
        request: Inbound,
    ) -> Inbound {
        match opened {
            Ok(()) => request,
            Err(error) => match error.0 {
                TransportErrorEnum::StreamLimit { limit } => {
                    let refusal = ErrorReply::new(ErrorCode::StreamLimit, error.to_string())
                        .with_detail(limit.into());
                    self.refuse(stream, &refusal);
                    Inbound::Refused {
                        stream,
                        code: ErrorCode::StreamLimit,
                    }
                }
                TransportErrorEnum::UnknownStream {
                    stream: transaction,
                } => self.refuse_with(
                    stream,
                    ErrorCode::Protocol,
                    &format!("no transaction is open on stream {transaction}"),
                ),
                _ => self.close(ErrorCode::Protocol, &error.to_string()),
            },
        }
    }

    fn refuse_rule(&mut self, stream: u64, rule: &ProtocolError) -> Inbound {
        self.refuse(stream, &ErrorReply::protocol(rule));
        Inbound::Refused {
            stream,
            code: ErrorCode::Protocol,
        }
    }

    fn refuse_with(&mut self, stream: u64, code: ErrorCode, message: &str) -> Inbound {
        self.refuse(stream, &ErrorReply::new(code, message));
        Inbound::Refused { stream, code }
    }

    /// The connection's last frame, an Error of `code` on stream 0, queued; the connection closes.
    fn close(&mut self, code: ErrorCode, message: &str) -> Inbound {
        let header = Header::new(self.cluster, Command::Error).with_flags(FLAG_LAST);
        if let Ok(frame) = ErrorReply::new(code, message).frame(header) {
            let _ = self.outbound.push_connection(frame);
        }
        self.phase = Phase::Closing;
        self.exchange = None;
        Inbound::Closing { code: Some(code) }
    }
}
