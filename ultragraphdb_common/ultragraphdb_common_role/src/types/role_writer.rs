/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::Header;

/// The sending half of a [`crate::RoleClient`], which numbers its frames after those the dial sent. A send dropped at its
/// `.await` may leave a frame written in part, so the half whose send was dropped is given up; a shutdown dropped leaves
/// the sending side open until the half is dropped.
#[derive(Debug)]
pub struct RoleWriter {
    connection: Connection<<Carrier as Splittable>::WriteHalf>,
    sent: u128,
}

impl RoleWriter {
    /// The half writing on `connection`, the frames before it `sent`.
    pub(crate) fn new(
        connection: Connection<<Carrier as Splittable>::WriteHalf>,
        sent: u128,
    ) -> Self {
        Self { connection, sent }
    }

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
