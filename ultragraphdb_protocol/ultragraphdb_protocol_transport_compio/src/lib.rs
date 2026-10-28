/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compio shell of `ultragraphdb_protocol_transport`: [`Connection`] does a connection's I/O over a compio
//! stream and obeys the transport's sans-I/O machines, the [`ultragraphdb_protocol_transport::Framer`] for every
//! frame read and the handshakes for Hello and Welcome; [`CompioSimStream`] is a simulated stream as a compio stream.
//! The server and the compio client share it.

mod types;

pub use crate::types::compio_sim_stream::CompioSimStream;
pub use crate::types::connection::Connection;
