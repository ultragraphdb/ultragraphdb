/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Rust client of the binary protocol, sans-I/O: [`ClientCore`] holds every rule of the protocol a client
//! follows, and a runtime's shell (`ultragraphdb_client_tokio`, `ultragraphdb_client_compio`) does its I/O.
//!
//! The core takes the frames the shell read, whole ([`ClientCore::receive`]), and the connection's failures
//! ([`ClientCore::disconnected`]); it queues the frames the shell writes ([`ClientCore::transmit`]); and it is told
//! the time with every call that may start a wait, never reading a clock, so the shell sleeps until
//! [`ClientCore::next_deadline`] and calls [`ClientCore::handle_timeout`]. Requests are the wire's own bodies, their
//! terms in the wire's term table; replies and result batches are read in place through the wire's views. A commit is
//! sent through a [`Session`] and retried only through it, with the same session and request number, across
//! reconnects and at most [`ClientConfig::commit_attempts`] times, so the server applies it once.

mod errors;
mod types;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::errors::client_error::{ClientError, ClientErrorEnum};
pub use crate::types::batch_poll::BatchPoll;
pub use crate::types::client_config::ClientConfig;
pub use crate::types::client_core::ClientCore;
pub use crate::types::committed::Committed;
pub use crate::types::pending::Pending;
pub use crate::types::reply::Reply;
pub use crate::types::result_stream::ResultStream;
pub use crate::types::session::Session;
pub use crate::types::statement::Statement;
pub use crate::types::transaction::Transaction;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::scripted_server::{ScriptedServer, ServerScript};
