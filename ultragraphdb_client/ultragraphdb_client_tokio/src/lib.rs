/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Rust client of the binary protocol on tokio: [`Client`] is the tokio shell of
//! `ultragraphdb_client_core`'s sans-I/O core, which holds every rule of the protocol; the shell connects through a
//! [`Connect`]or, writes what the core queues, reads frames as the transport's framer asks, straight into pooled
//! buffers, and sleeps no longer than the core's next deadline. Over [`TcpConnector`] its futures are `Send`, so it
//! runs on a multi-threaded runtime; [`TokioSimStream`] runs it over the simulated network; [`TlsDial`] over any other
//! connector wraps each connection in a [`TokioTlsStream`], verified against a [`TlsTrust`].

mod traits;
mod types;

pub use crate::traits::connect::Connect;
pub use crate::types::client::Client;
pub use crate::types::tcp_connector::TcpConnector;
pub use crate::types::tls_dial::TlsDial;
pub use crate::types::tokio_sim_stream::TokioSimStream;
pub use crate::types::tokio_tls_stream::TokioTlsStream;
pub use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientError, ClientErrorEnum, Committed, Reply, ResultStream, Session,
    Statement, Transaction,
};
pub use ultragraphdb_protocol_transport::{Anonymous, Login};
pub use ultragraphdb_protocol_wire::{ReadLevel, RefusedRequest, RunsAgain};
pub use ultragraphdb_transport_tls::{TlsConnector, TlsError, TlsTrust};
