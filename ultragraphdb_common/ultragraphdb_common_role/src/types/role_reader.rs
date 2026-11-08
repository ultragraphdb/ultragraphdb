/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;
use ultragraphdb_protocol_transport::{BufferPool, ReceivedFrame, TransportError};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};

/// The receiving half of a [`crate::RoleClient`], whose read pending holds up no write. A read dropped at its `.await` cuts
/// the frame in hand, its bytes read so far lost, so the half whose read was dropped is given up.
#[derive(Debug)]
pub struct RoleReader {
    connection: Connection<<Carrier as Splittable>::ReadHalf>,
    pool: BufferPool,
}

impl RoleReader {
    /// The half reading `connection` into a buffer of `pool`.
    pub(crate) fn new(
        connection: Connection<<Carrier as Splittable>::ReadHalf>,
        pool: BufferPool,
    ) -> Self {
        Self { connection, pool }
    }

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
