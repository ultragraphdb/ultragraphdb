/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;
use ultragraphdb_protocol_transport::Framer;
use ultragraphdb_protocol_transport_compio::Connection;
use ultragraphdb_protocol_wire::Welcome;

use super::RoleClient;
use crate::types::role_reader::RoleReader;
use crate::types::role_writer::RoleWriter;

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
            RoleReader::new(Connection::new(read, framer), pool),
            RoleWriter::new(Connection::new(write, framer), sent),
            welcome,
        )
    }
}
