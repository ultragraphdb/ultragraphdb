/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Rust client of the binary protocol on compio: [`Client`] is the compio shell of `ultragraphdb_client_core`'s
//! sans-I/O core, which holds every rule of the protocol. Its I/O is the transport's compio
//! [`ultragraphdb_protocol_transport_compio::Connection`], which reads every frame as the transport's framer asks into
//! a pooled buffer, with owned-buffer reads (io_uring on Linux). It runs on a compio runtime, thread per core; over
//! [`ultragraphdb_protocol_transport_compio::CompioSimStream`] it runs over the simulated network.

mod traits;
mod types;

pub use crate::traits::connect::Connect;
pub use crate::types::client::Client;
pub use crate::types::tcp_connector::TcpConnector;
pub use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientError, ClientErrorEnum, Committed, Reply, ResultStream, Session,
    Statement, Transaction,
};
pub use ultragraphdb_protocol_transport::{Anonymous, Login};
