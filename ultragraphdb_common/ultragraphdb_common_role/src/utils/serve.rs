/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;
use std::pin::pin;
use std::task::Poll;
use std::time::{Duration, Instant};

use compio::io::util::Splittable;
use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::{
    Credentials, DEFAULT_ITERATIONS, MECHANISM, ScramAuthenticator, system_nonce,
};
use ultragraphdb_protocol_transport::{
    AUTHENTICATION_TIMEOUT, Authenticator, BufferPool, Endpoint, Framer, Inbound, ReceivedFrame,
    ServerConnection, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_transport_compio::Connection;
use ultragraphdb_protocol_wire::{
    AuthenticateView, BodyView, Command, CreditView, ErrorCode, ErrorReply, MAX_FRAME_SIZE,
    ROLE_MAX_STREAMS, Welcome,
};

use crate::traits::session::{Answer, Session};
use crate::types::connection_bound::ConnectionBound;
use crate::types::open_requests::OpenRequests;
use crate::types::parked_bytes::ParkedBytes;

const PROTOCOL: u16 = 1;
const RELEASE: u32 = 1;

#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the clock and tells the connection, which reads none"
)]
fn now() -> Instant {
    Instant::now()
}

/// How one kind of connection is served: its cluster, the endpoint it arrived at, and the secret an unknown name's
/// verifier derives from, at the iterations a known name's took, so a refused name costs what a wrong password costs.
#[derive(Debug, Clone)]
pub struct Server {
    cluster: u64,
    endpoint: Endpoint,
    name: &'static str,
    unknown: [u8; 32],
    iterations: u32,
    open: OpenRequests,
    bound: Option<ConnectionBound>,
    welcome: fn(Welcome) -> Welcome,
}

impl Server {
    /// A server of `cluster`, `name` in its Welcome, at `endpoint`.
    ///
    /// # Errors
    ///
    /// [`RoleError::unavailable`] when the operating system gives no random bytes.
    pub fn new(cluster: u64, name: &'static str, endpoint: Endpoint) -> Result<Self, RoleError> {
        let mut unknown = [0u8; 32];
        getrandom::fill(&mut unknown)
            .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
        Ok(Self {
            cluster,
            endpoint,
            name,
            unknown,
            iterations: DEFAULT_ITERATIONS,
            open: OpenRequests::default(),
            bound: None,
            welcome: |welcome| welcome,
        })
    }

    /// The server answering unknown names with verifiers derived from `secret`, the one the metadata group holds, so every
    /// core and restart answers one name with one salt.
    #[must_use]
    pub fn with_unknown_secret(mut self, secret: [u8; 32]) -> Self {
        self.unknown = secret;
        self
    }

    /// The server holding at most what `bound` allows: a connection past it is refused in place of its Welcome.
    #[must_use]
    pub fn with_bound(mut self, bound: ConnectionBound) -> Self {
        self.bound = Some(bound);
        self
    }

    /// The bound of connections it holds, if it has one: what a transport layered beneath the protocol, TLS, counts its
    /// handshakes against before a connection reaches [`serve_connection`].
    #[must_use]
    pub fn bound(&self) -> Option<&ConnectionBound> {
        self.bound.as_ref()
    }

    /// The server's Welcome as `welcome` makes it of the default limits.
    #[must_use]
    pub fn with_welcome(mut self, welcome: fn(Welcome) -> Welcome) -> Self {
        self.welcome = welcome;
        self
    }

    /// The server answering unknown names with verifiers of `iterations`, the count its credentials hold.
    #[must_use]
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }

    /// The server counting what its connections hold open in `open`, and refusing new requests once that says it stops.
    #[must_use]
    pub fn with_open(mut self, open: OpenRequests) -> Self {
        self.open = open;
        self
    }

    /// The machine of a connection accepted at `at`, authenticating against `credentials`, or `None` when no nonce can
    /// be drawn for it.
    fn machine<C: Credentials + Clone>(
        &self,
        credentials: C,
        at: Instant,
    ) -> Option<ServerConnection<impl Authenticator>> {
        let fallback = system_nonce().ok()?;
        // A later exchange draws afresh; a source failing by then repeats this connection's nonce, not another's.
        let nonces = move || system_nonce().unwrap_or_else(|_| fallback.clone());
        let authenticator = ScramAuthenticator::new(credentials, nonces, self.unknown)
            .with_iterations(self.iterations);
        // The default limits are a client's. A role connection carries what the frontend already bounded on its clients'
        // connections, so a role endpoint announces the protocol's largest frame and its most streams (`PROTOCOL.md` §11.12).
        let limits =
            (self.welcome)(Welcome::new(PROTOCOL, RELEASE, self.name)).with_mechanisms([MECHANISM]);
        let welcome = if self.endpoint == Endpoint::Client {
            limits
        } else {
            limits
                .with_max_frame(MAX_FRAME_SIZE as u32) // 2^20, lossless
                .with_max_streams(ROLE_MAX_STREAMS)
        };
        let connection = ServerConnection::new(self.cluster, welcome, authenticator, at)
            .with_endpoint(self.endpoint);
        Some(if self.endpoint == Endpoint::Client {
            connection
        } else {
            // A role connection may rest between requests for as long as a Subscribe's closes take.
            connection.with_limits(AUTHENTICATION_TIMEOUT, Duration::MAX)
        })
    }
}

/// Serves one connection until it closes: its handshake, its authentication against `credentials` and its endpoint's
/// rules in the connection's machine, and what those pass on in `session`. A failing stream ends the connection, which
/// holds nothing of anyone else's.
///
/// A dropped future ends the connection as a failing stream does, without its shutdown: a frame read or written in part is
/// cut, `session` is dropped with what it holds, and the connection's place under the server's bound and its open
/// request are let go.
pub async fn serve_connection<S, C, H>(stream: S, server: &Server, credentials: C, mut session: H)
where
    S: Splittable,
    S::ReadHalf: AsyncRead,
    S::WriteHalf: AsyncWrite,
    C: Credentials + Clone,
    H: Session,
{
    let Some(machine) = server.machine(credentials, now()) else {
        tracing::warn!("no nonce could be drawn for a connection");
        return;
    };
    // Held until the connection ends, whichever way it goes on from here.
    let held = server.bound.as_ref().map(ConnectionBound::enter);
    let mut machine = match &held {
        Some(entered) if !entered.within() => machine.past_connection_bound(entered.limit()),
        Some(_) | None => machine,
    };
    let (reader, writer) = stream.split();
    let mut reader = Connection::new(reader, Framer::from_client(server.cluster, MAX_FRAME_SIZE));
    let mut writer = Connection::new(writer, Framer::new(server.cluster, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let ended = run(
        &mut reader,
        &mut writer,
        &mut machine,
        &pool,
        &mut session,
        &server.open,
    )
    .await;
    if let Err(error) = ended {
        tracing::debug!(%error, "a connection ended");
    }
    writer.shutdown().await;
}

/// The connection's event loop. A dropped future lets go of the frame in hand and its open request, and leaves the
/// connection to its end, as [`serve_connection`] says.
async fn run<R, W, A, H>(
    reader: &mut Connection<R>,
    writer: &mut Connection<W>,
    machine: &mut ServerConnection<A>,
    pool: &BufferPool,
    session: &mut H,
    open: &OpenRequests,
) -> Result<(), TransportError>
where
    R: AsyncRead,
    W: AsyncWrite,
    A: Authenticator,
    H: Session,
{
    let parked = session.parked();
    // An event loop: the connection machine's close or a transport error ends it (Ultra rule 2).
    while !machine.is_closing() {
        let frame = next_frame(reader, writer, machine, pool, session, parked.as_ref()).await?;
        // A frame stays open until what it asks is on the wire, past the flush below: that is what a stop waits for.
        let _open = open.enter();
        match frame {
            Some(Ok(received)) => take(reader, machine, session, received, open).await?,
            Some(Err(error)) => {
                let TransportErrorEnum::Protocol { error: refusal } = &error.0 else {
                    return Err(error);
                };
                let _closing = machine.refuse_connection(refusal)?;
            }
            None => {}
        }
        flush(writer, machine, session).await?;
    }
    Ok(())
}

/// The frame read next, sending what the session pushes meanwhile; the read stays pinned across those sends, so a frame
/// is never cut short, and starts only once `parked` has room, which only a frame taken can use up. `None` once a deadline passed
/// and the connection's machine has taken it. A dropped future drops the pinned read with it, cutting a frame read in part,
/// and may cut a pushed frame being written: the connection ends with it.
async fn next_frame<R, W, A, H>(
    reader: &mut Connection<R>,
    writer: &mut Connection<W>,
    machine: &mut ServerConnection<A>,
    pool: &BufferPool,
    session: &mut H,
    parked: Option<&ParkedBytes>,
) -> Result<Option<Result<ReceivedFrame, TransportError>>, TransportError>
where
    R: AsyncRead,
    W: AsyncWrite,
    A: Authenticator,
    H: Session,
{
    let mut read = pin!(async {
        if let Some(parked) = parked {
            parked.room().await;
        }
        reader.read_frame(pool).await
    });
    loop {
        let wait = machine
            .next_deadline()
            .map(|deadline| deadline.saturating_duration_since(now()));
        // The push future borrows the session, so its scope ends before a flush tells the session of room.
        let event = {
            let mut deadline = pin!(async {
                match wait {
                    Some(wait) => compio::time::sleep(wait).await,
                    None => std::future::pending().await,
                }
            });
            let mut push = pin!(session.next_push());
            poll_fn(|cx| {
                if let Poll::Ready(frame) = read.as_mut().poll(cx) {
                    return Poll::Ready(Event::Frame(frame));
                }
                if let Poll::Ready(pushed) = push.as_mut().poll(cx) {
                    return Poll::Ready(Event::Push(pushed));
                }
                deadline.as_mut().poll(cx).map(|()| Event::Deadline)
            })
            .await
        };
        match event {
            Event::Frame(frame) => return Ok(Some(frame)),
            Event::Push((stream, frame)) => {
                // A reply to a stream the peer cancelled meanwhile has no one to go to.
                if machine.outbound().is_open(stream) {
                    machine.outbound().push(stream, frame)?;
                    flush(writer, machine, session).await?;
                }
            }
            Event::Deadline => {
                let _closing = machine.handle_timeout(now())?;
                if machine.is_closing() {
                    return Ok(None);
                }
            }
        }
    }
}

/// What woke a connection.
enum Event {
    Frame(Result<ReceivedFrame, TransportError>),
    Push((u64, Vec<u8>)),
    Deadline,
}

/// `received` taken by the machine, and a request it hands on answered. A future dropped at the session's `.await` drops
/// `received` unanswered, and leaves what the session's own future leaves.
async fn take<R, A, H>(
    reader: &mut Connection<R>,
    machine: &mut ServerConnection<A>,
    session: &mut H,
    received: ReceivedFrame,
    open: &OpenRequests,
) -> Result<(), TransportError>
where
    A: Authenticator,
    H: Session,
{
    if let Ok(view) = received.view()
        && view.command() == Command::Authenticate
        && view.header().stream() == 0
        && let Ok(message) = AuthenticateView::from_frame(&view)
        && message.step() == 0
    {
        session.authenticating(message.message()).await;
    }
    let credit = received.view().ok().and_then(|view| {
        (view.command() == Command::Credit)
            .then(|| {
                let frames = CreditView::from_frame(&view).ok()?.frames();
                Some((view.header().stream(), frames))
            })
            .flatten()
    });
    match machine.receive(&received, now())? {
        Inbound::Welcomed {
            protocol,
            max_frame,
        } => {
            reader.framer_mut().set_protocol(protocol);
            session.welcomed(max_frame);
        }
        Inbound::Authenticated { principal } => session.authenticated(principal),
        Inbound::Closing {
            code: Some(ErrorCode::AuthenticationFailed),
        } => session.authentication_refused(),
        Inbound::Handled => {
            if let Some((stream, frames)) = credit {
                session.credited(stream, frames);
            }
        }
        // A CloseRequest's stream ends with it, a refusal included (`PROTOCOL.md` §13.16), so its refusal is dropped.
        Inbound::Request { stream, command } if open.is_stopping() => {
            let refusal = ErrorReply::new(ErrorCode::Unavailable, "this process is stopping");
            refuse(machine, stream, command, &refusal)?;
        }
        Inbound::Request { stream, command } => match session.answer(stream, received).await {
            Answer::Reply(reply) => machine.outbound().push(stream, reply)?,
            Answer::Open => {}
            Answer::Done => {
                let _ended = machine.outbound().cancel(stream);
            }
            Answer::Refuse(refusal) => refuse(machine, stream, command, &refusal)?,
        },
        Inbound::Cancelled { stream, .. } => session.cancelled(stream),
        Inbound::Refused { .. } | Inbound::Closing { .. } => {}
    }
    Ok(())
}

/// `refusal` of the `command` on `stream`: sent on the stream, or for a CloseRequest, which no frame answers, logged and its
/// stream ended.
fn refuse<A: Authenticator>(
    machine: &mut ServerConnection<A>,
    stream: u64,
    command: Command,
    refusal: &ErrorReply,
) -> Result<(), TransportError> {
    if command == Command::CloseRequest {
        tracing::warn!(
            stream,
            code = refusal.code().number(),
            reason = refusal.message(),
            "a CloseRequest was refused"
        );
        let _ended = machine.outbound().cancel(stream);
        return Ok(());
    }
    machine.refuse(stream, refusal)
}

/// Writes every frame the machine lets go, then tells the session of each stream with room for its next frame, and of each
/// whose frame left with none. A future dropped at a write may cut that frame, which the machine has let go already: the
/// connection ends with it.
async fn flush<W, A, H>(
    writer: &mut Connection<W>,
    machine: &mut ServerConnection<A>,
    session: &mut H,
) -> Result<(), TransportError>
where
    W: AsyncWrite,
    A: Authenticator,
    H: Session,
{
    while let Some((_, frame)) = machine.next_frame() {
        let _sent = writer.write_frame(frame).await?;
    }
    machine
        .outbound()
        .take_ready()
        .into_iter()
        .for_each(|stream| session.ready(stream));
    machine
        .outbound()
        .take_left()
        .into_iter()
        .for_each(|stream| session.left(stream));
    Ok(())
}
