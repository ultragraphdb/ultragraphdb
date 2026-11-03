/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compio shell of `ultragraphdb_protocol_transport`: [`Connection`] does a connection's I/O over a compio
//! stream and obeys the transport's sans-I/O machines, the [`ultragraphdb_protocol_transport::Framer`] for every
//! frame read and the handshakes for Hello and Welcome; [`CompioSimStream`] is a simulated stream as a compio stream.
//! The server and the compio client share it. A [`Carrier`] is the stream beneath, a TCP or a Unix-domain socket as
//! the scheme of its `Address` chose, dialled by [`Carrier::connect`] or taken from a [`Listener`]; a
//! [`CompioTlsStream`] drives a TLS session over any of them.

mod types;

pub use crate::types::carrier::Carrier;
pub use crate::types::compio_sim_stream::CompioSimStream;
pub use crate::types::compio_tls_stream::CompioTlsStream;
pub use crate::types::connection::Connection;
pub use crate::types::listener::Listener;
