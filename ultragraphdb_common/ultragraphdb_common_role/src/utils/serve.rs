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
use ultragraphdb_protocol_wire::{AuthenticateView, BodyView, Command, MAX_FRAME_SIZE, Welcome};

use crate::traits::session::{Answer, Session};

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
        })
    }

    /// The server answering unknown names with verifiers of `iterations`, the count its credentials hold.
    #[must_use]
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
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
        let welcome = Welcome::new(PROTOCOL, RELEASE, self.name).with_mechanisms([MECHANISM]);
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
pub async fn serve_connection<S, C, H>(stream: S, server: &Server, credentials: C, mut session: H)
where
    S: Splittable,
    S::ReadHalf: AsyncRead,
    S::WriteHalf: AsyncWrite,
    C: Credentials + Clone,
    H: Session,
{
    let Some(mut machine) = server.machine(credentials, now()) else {
        tracing::warn!("no nonce could be drawn for a connection");
        return;
    };
    let (reader, writer) = stream.split();
    let mut reader = Connection::new(reader, Framer::from_client(server.cluster, MAX_FRAME_SIZE));
    let mut writer = Connection::new(writer, Framer::new(server.cluster, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    if let Err(error) = run(&mut reader, &mut writer, &mut machine, &pool, &mut session).await {
        tracing::debug!(%error, "a connection ended");
    }
    writer.shutdown().await;
}

async fn run<R, W, A, H>(
    reader: &mut Connection<R>,
    writer: &mut Connection<W>,
    machine: &mut ServerConnection<A>,
    pool: &BufferPool,
    session: &mut H,
) -> Result<(), TransportError>
where
    R: AsyncRead,
    W: AsyncWrite,
    A: Authenticator,
    H: Session,
{
    while !machine.is_closing() {
        match next_frame(reader, writer, machine, pool, session).await? {
            Some(Ok(received)) => take(reader, machine, session, received).await?,
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
/// is never cut short. `None` once a deadline passed and the connection's machine has taken it.
async fn next_frame<R, W, A, H>(
    reader: &mut Connection<R>,
    writer: &mut Connection<W>,
    machine: &mut ServerConnection<A>,
    pool: &BufferPool,
    session: &mut H,
) -> Result<Option<Result<ReceivedFrame, TransportError>>, TransportError>
where
    R: AsyncRead,
    W: AsyncWrite,
    A: Authenticator,
    H: Session,
{
    let mut read = pin!(reader.read_frame(pool));
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

/// `received` taken by the machine, and a request it hands on answered.
async fn take<R, A, H>(
    reader: &mut Connection<R>,
    machine: &mut ServerConnection<A>,
    session: &mut H,
    received: ReceivedFrame,
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
    match machine.receive(&received, now())? {
        Inbound::Welcomed {
            protocol,
            max_frame,
        } => {
            reader.framer_mut().set_protocol(protocol);
            session.welcomed(max_frame);
        }
        Inbound::Request { stream, .. } => match session.answer(stream, received).await {
            Answer::Reply(reply) => machine.outbound().push(stream, reply)?,
            Answer::Open => {}
            Answer::Done => {
                let _ended = machine.outbound().cancel(stream);
            }
            Answer::Refuse(refusal) => machine.refuse(stream, &refusal)?,
        },
        Inbound::Cancelled { stream, .. } => session.cancelled(stream),
        Inbound::Handled
        | Inbound::Authenticated { .. }
        | Inbound::Refused { .. }
        | Inbound::Closing { .. } => {}
    }
    Ok(())
}

/// Writes every frame the machine lets go, then tells the session of each stream with room for its next frame, and of each
/// whose frame left with none.
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
