/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;
use ultragraphdb_protocol_transport::{BufferPool, Framer, ReceivedFrame, TransportError};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::{Header, Welcome};

use super::RoleClient;

/// The receiving half of a [`RoleClient`], whose read pending holds up no write.
#[derive(Debug)]
pub struct RoleReader {
    connection: Connection<<Carrier as Splittable>::ReadHalf>,
    pool: BufferPool,
}

impl RoleReader {
    /// The next frame the peer sends, which holds this half's one buffer until it is dropped.
    ///
    /// # Errors
    ///
    /// As reading fails.
    pub async fn next_frame(&mut self) -> Result<ReceivedFrame, TransportError> {
        self.connection.read_frame(&self.pool).await
    }

    /// The next frame the peer sends, read into a buffer of `pool`, for a caller holding several frames at once.
    ///
    /// # Errors
    ///
    /// As reading fails, and [`TransportError::pool_exhausted`] with no buffer of `pool` free.
    pub async fn next_frame_in(
        &mut self,
        pool: &BufferPool,
    ) -> Result<ReceivedFrame, TransportError> {
        self.connection.read_frame(pool).await
    }
}

/// The sending half of a [`RoleClient`], which numbers its frames after those the dial sent.
#[derive(Debug)]
pub struct RoleWriter {
    connection: Connection<<Carrier as Splittable>::WriteHalf>,
    sent: u128,
}

impl RoleWriter {
    /// Writes `frame`, numbered after the frames before it.
    ///
    /// # Errors
    ///
    /// As writing fails.
    pub async fn send(&mut self, mut frame: Vec<u8>) -> Result<(), TransportError> {
        self.sent += 1;
        Header::stamp_sequence(&mut frame, self.sent).map_err(TransportError::protocol)?;
        let _sent = self.connection.write_frame(frame).await?;
        Ok(())
    }

    /// Shuts down the sending side.
    pub async fn shutdown(&mut self) {
        self.connection.shutdown().await;
    }
}

impl RoleClient {
    /// The dialled connection's two halves and its peer's Welcome, past its authentication: a read and a write may then wait at
    /// once. Streams are numbered by whoever holds the writer, from above those this client opened.
    #[must_use]
    pub fn split(self) -> (RoleReader, RoleWriter, Welcome) {
        let Self {
            mut connection,
            pool,
            cluster: _,
            sent,
            stream: _,
            welcome,
        } = self;
        let framer: Framer = *connection.framer_mut();
        let (read, write) = connection.into_inner().split();
        (
            RoleReader {
                connection: Connection::new(read, framer),
                pool,
            },
            RoleWriter {
                connection: Connection::new(write, framer),
                sent,
            },
            welcome,
        )
    }
}
