/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::{
    Credentials, DEFAULT_ITERATIONS, MECHANISM, ScramAuthenticator, system_nonce,
};
use ultragraphdb_protocol_transport::{
    AUTHENTICATION_TIMEOUT, Address, Authenticator, BufferPool, Endpoint, Framer, Inbound,
    ServerConnection, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_transport_compio::{Connection, Listener};
use ultragraphdb_protocol_wire::{ErrorReply, FrameView, MAX_FRAME_SIZE, Welcome};

const PROTOCOL: u16 = 1;
const RELEASE: u32 = 1;

/// Mode 0600: a role's socket opens to the user the roles run as (`PROTOCOL.md` §11.12).
const SOCKET_MODE: u32 = 0o600;

#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the clock and tells the connection, which reads none"
)]
fn now() -> Instant {
    Instant::now()
}

/// Listens at the role address `text`. A Unix socket a process before this one left behind is replaced, and the new one
/// is set to mode 0600.
///
/// # Errors
///
/// [`RoleError::configuration`] for an address that does not parse, and [`RoleError::unavailable`] when it cannot be
/// listened at.
pub async fn listen(text: &str) -> Result<Listener, RoleError> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let address = Address::parse(text)
        .map_err(|error| RoleError::configuration(format!("the address {text}: {error}")))?;
    let unavailable =
        |why: &dyn std::fmt::Display| RoleError::unavailable(format!("listening at {text}: {why}"));
    let path = address.path();
    if let Some(path) = path {
        let stale = std::fs::symlink_metadata(path).is_ok_and(|file| file.file_type().is_socket());
        if stale {
            std::fs::remove_file(path).map_err(|error| unavailable(&error))?;
        }
    }
    let listener = Listener::bind(&address)
        .await
        .map_err(|error| unavailable(&error))?;
    if let Some(path) = path {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
            .map_err(|error| unavailable(&error))?;
    }
    Ok(listener)
}

/// How one kind of connection is served: its cluster, the endpoint it arrived at, who may authenticate there, and the
/// verifier-deriving secret an unknown name is answered with, so a refused name costs what a wrong password costs.
#[derive(Debug, Clone)]
pub struct Server<C> {
    cluster: u64,
    endpoint: Endpoint,
    name: &'static str,
    credentials: C,
    unknown: [u8; 32],
    iterations: u32,
}

impl<C: Credentials + Clone> Server<C> {
    /// A server of `cluster`, `name` in its Welcome, at `endpoint`, authenticating against `credentials`.
    ///
    /// # Errors
    ///
    /// [`RoleError::unavailable`] when the operating system gives no random bytes.
    pub fn new(
        cluster: u64,
        name: &'static str,
        endpoint: Endpoint,
        credentials: C,
    ) -> Result<Self, RoleError> {
        let mut unknown = [0u8; 32];
        getrandom::fill(&mut unknown)
            .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
        Ok(Self {
            cluster,
            endpoint,
            name,
            credentials,
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

    /// The connection a stream accepted at `now` is, or `None` when no nonce can be drawn for it.
    fn connection(&self, now: Instant) -> Option<ServerConnection<impl Authenticator>> {
        let fallback = system_nonce().ok()?;
        // A later exchange draws afresh; a source failing by then repeats this connection's nonce, not another's.
        let nonces = move || system_nonce().unwrap_or_else(|_| fallback.clone());
        let authenticator = ScramAuthenticator::new(self.credentials.clone(), nonces, self.unknown)
            .with_iterations(self.iterations);
        let welcome = Welcome::new(PROTOCOL, RELEASE, self.name).with_mechanisms([MECHANISM]);
        let connection = ServerConnection::new(self.cluster, welcome, authenticator, now)
            .with_endpoint(self.endpoint);
        Some(if self.endpoint == Endpoint::Client {
            connection
        } else {
            // A role connection may rest between requests for as long as a Subscribe's closes take.
            connection.with_limits(AUTHENTICATION_TIMEOUT, Duration::MAX)
        })
    }
}

/// Serves one connection until it closes: its handshake, its authentication and the rules of its endpoint in the
/// connection's machine, and each request it passes on answered by `answer`, given its stream and its frame, with the
/// reply's frame or a refusal. A failing stream ends the connection, which holds nothing of anyone else's.
pub async fn serve_connection<S, C, H>(stream: S, server: &Server<C>, mut answer: H)
where
    S: AsyncRead + AsyncWrite,
    C: Credentials + Clone,
    H: FnMut(u64, &FrameView<'_>) -> Result<Vec<u8>, ErrorReply>,
{
    let Some(mut machine) = server.connection(now()) else {
        tracing::warn!("no nonce could be drawn for a connection");
        return;
    };
    let mut wire = Connection::new(stream, Framer::from_client(server.cluster, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    if let Err(error) = run(&mut wire, &mut machine, &pool, &mut answer).await {
        tracing::debug!(%error, "a connection ended");
    }
    wire.shutdown().await;
}

async fn run<S, A, H>(
    wire: &mut Connection<S>,
    machine: &mut ServerConnection<A>,
    pool: &BufferPool,
    answer: &mut H,
) -> Result<(), TransportError>
where
    S: AsyncRead + AsyncWrite,
    A: Authenticator,
    H: FnMut(u64, &FrameView<'_>) -> Result<Vec<u8>, ErrorReply>,
{
    while !machine.is_closing() {
        let read = read_until_deadline(wire, machine, pool).await;
        match read {
            Ok(Some(received)) => {
                let inbound = machine.receive(&received, now())?;
                match inbound {
                    Inbound::Welcomed { protocol } => wire.framer_mut().set_protocol(protocol),
                    Inbound::Request { stream, .. } => {
                        let view = received.view().map_err(TransportError::protocol)?;
                        match answer(stream, &view) {
                            Ok(reply) => machine.outbound().push(stream, reply)?,
                            Err(refusal) => machine.refuse(stream, &refusal)?,
                        }
                    }
                    Inbound::Handled
                    | Inbound::Authenticated { .. }
                    | Inbound::Cancelled { .. }
                    | Inbound::Refused { .. }
                    | Inbound::Closing { .. } => {}
                }
            }
            Ok(None) => {
                let _closing = machine.handle_timeout(now())?;
            }
            Err(error) => {
                let TransportErrorEnum::Protocol { error: refusal } = &error.0 else {
                    return Err(error);
                };
                let _closing = machine.refuse_connection(refusal)?;
            }
        }
        while let Some((_, frame)) = machine.next_frame() {
            let _sent = wire.write_frame(frame).await?;
        }
    }
    Ok(())
}

/// The next frame, or `None` once the connection's next deadline passed. A deadline that passes ends in the
/// connection's close, so a frame cut short by it is never read further.
async fn read_until_deadline<S, A>(
    wire: &mut Connection<S>,
    machine: &ServerConnection<A>,
    pool: &BufferPool,
) -> Result<Option<ultragraphdb_protocol_transport::ReceivedFrame>, TransportError>
where
    S: AsyncRead + AsyncWrite,
    A: Authenticator,
{
    let Some(deadline) = machine.next_deadline() else {
        return wire.read_frame(pool).await.map(Some);
    };
    let wait = deadline.saturating_duration_since(now());
    match compio::time::timeout(wait, wire.read_frame(pool)).await {
        Ok(read) => read.map(Some),
        Err(_) => Ok(None),
    }
}
