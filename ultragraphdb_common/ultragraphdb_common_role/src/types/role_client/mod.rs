/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{ScramClient, system_nonce};
use ultragraphdb_protocol_transport::{
    Address, AuthenticationStep, BufferPool, ClientAuthentication, Framer, ReceivedFrame,
    TransportError,
};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::{
    Authenticate, BodyBuilder, BodyView, Command, ErrorReplyView, Header, Hello, MAX_FRAME_SIZE,
};

/// The protocol version this release speaks, and its release number.
const PROTOCOL: u16 = 1;
const RELEASE: u32 = 1;

/// The messages an Authenticate exchange of SCRAM-SHA-256 takes from the server.
const EXCHANGE_STEPS: usize = 2;

/// The side of a role connection that dialled: past [`RoleClient::dial`], which authenticates before anything else is
/// sent, so a role refused by its peer has no connection to send a role command on.
#[derive(Debug)]
pub struct RoleClient {
    connection: Connection<Carrier>,
    pool: BufferPool,
    cluster: u64,
    sent: u128,
    stream: u64,
}

impl RoleClient {
    /// Dials `address` of `cluster` as the role `name` holding `password`, and authenticates. A role verifies the peer in
    /// turn, by the server's final message.
    ///
    /// # Errors
    ///
    /// [`TransportError`] as dialling, the handshake or the exchange fails: a refusal of the secret is
    /// [`TransportError::refused`] with code 19.
    pub async fn dial(
        address: &Address,
        cluster: u64,
        name: &str,
        password: &str,
    ) -> Result<Self, TransportError> {
        let carrier = Carrier::connect(address).await?;
        let mut client = Self {
            connection: Connection::new(carrier, Framer::new(cluster, MAX_FRAME_SIZE)),
            pool: BufferPool::new(1, MAX_FRAME_SIZE),
            cluster,
            sent: 0,
            stream: 0,
        };
        let hello = Hello::new(PROTOCOL, PROTOCOL, RELEASE, MAX_FRAME_SIZE as u32, name);
        let _welcome = client.connection.hello(&client.pool, &hello).await?;
        client.sent = 1;
        client.authenticate(name, password).await?;
        Ok(client)
    }

    /// Sends `body` on a stream of its own and reads its one reply, the reply's frame, which holds the connection's one
    /// buffer until it is dropped.
    ///
    /// # Errors
    ///
    /// [`TransportError::refused`] when the peer answers with an Error, and as reading or writing fails.
    pub async fn request<B: BodyBuilder>(
        &mut self,
        body: &B,
    ) -> Result<ReceivedFrame, TransportError> {
        self.stream += 1;
        let header = Header::new(self.cluster, B::COMMAND).with_stream(self.stream);
        let frame = body
            .frame(header)
            .map_err(|error| TransportError::frame_not_built(B::COMMAND.number(), error))?;
        self.send(frame).await?;
        let reply = self.connection.read_frame(&self.pool).await?;
        let view = reply.view().map_err(TransportError::protocol)?;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view).map_err(TransportError::protocol)?;
            return Err(TransportError::refused(error.code(), error.message()));
        }
        Ok(reply)
    }

    /// Sends `body` on a stream of its own, which ends with it: for a request no frame answers, a CloseRequest's.
    ///
    /// # Errors
    ///
    /// As writing fails.
    pub async fn send_only<B: BodyBuilder>(&mut self, body: &B) -> Result<(), TransportError> {
        self.stream += 1;
        let header = Header::new(self.cluster, B::COMMAND).with_stream(self.stream);
        let frame = body
            .frame(header)
            .map_err(|error| TransportError::frame_not_built(B::COMMAND.number(), error))?;
        self.send(frame).await
    }

    /// The stream the last request opened, for what follows on it, as a Bind's transaction does.
    #[must_use]
    pub fn last_stream(&self) -> u64 {
        self.stream
    }

    /// Sends `body` on the open stream `stream`.
    ///
    /// # Errors
    ///
    /// As writing fails.
    pub async fn send_on<B: BodyBuilder>(
        &mut self,
        stream: u64,
        body: &B,
    ) -> Result<(), TransportError> {
        let header = Header::new(self.cluster, B::COMMAND).with_stream(stream);
        let frame = body
            .frame(header)
            .map_err(|error| TransportError::frame_not_built(B::COMMAND.number(), error))?;
        self.send(frame).await
    }

    /// The next frame the peer sends unasked, such as the next close on a Subscribe's stream. It holds the connection's
    /// one buffer until it is dropped.
    ///
    /// # Errors
    ///
    /// As reading fails.
    pub async fn next_frame(&mut self) -> Result<ReceivedFrame, TransportError> {
        self.connection.read_frame(&self.pool).await
    }

    async fn authenticate(&mut self, name: &str, password: &str) -> Result<(), TransportError> {
        let failed =
            |error: &dyn std::fmt::Display| TransportError::authentication(error.to_string());
        let nonce = system_nonce().map_err(|error| failed(&error))?;
        let mechanism = ScramClient::new(name, password, &nonce).map_err(|error| failed(&error))?;
        let mut exchange = ClientAuthentication::new(mechanism);
        let first = exchange.start()?;
        self.send_authenticate(&first).await?;
        for _ in 0..EXCHANGE_STEPS {
            let answer = self.connection.read_frame(&self.pool).await?;
            let view = answer.view().map_err(TransportError::protocol)?;
            match exchange.receive(&view)? {
                AuthenticationStep::Send(message) => self.send_authenticate(&message).await?,
                AuthenticationStep::Done { .. } => return Ok(()),
            }
        }
        Err(TransportError::authentication("the exchange did not end"))
    }

    async fn send_authenticate(&mut self, message: &Authenticate) -> Result<(), TransportError> {
        let frame = message
            .frame(Header::new(self.cluster, Command::Authenticate))
            .map_err(|error| {
                TransportError::frame_not_built(Command::Authenticate.number(), error)
            })?;
        self.send(frame).await
    }

    /// Writes `frame`, numbered after the frames before it.
    async fn send(&mut self, mut frame: Vec<u8>) -> Result<(), TransportError> {
        self.sent += 1;
        Header::stamp_sequence(&mut frame, self.sent).map_err(TransportError::protocol)?;
        let _sent = self.connection.write_frame(frame).await?;
        Ok(())
    }
}
