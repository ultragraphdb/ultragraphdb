/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Connections over compio: frames read into pooled buffers, the handshake, streams with credit and cancellation, and
//! a simulated network for the node simulator.
//!
//! A [`Connection`] reads a frame header first, verified for its cluster before its `size` is trusted, then the body
//! into the same [`PooledBuffer`] from the core's [`BufferPool`]; the frame's view is checked once from the
//! [`ReceivedFrame`]. [`Outbound`] schedules a server's frames across a connection's streams: replies go out as they
//! are pushed, Result frames only while their stream holds credit, and a cancelled stream sends nothing more. The
//! [`SimNetwork`] carries frames between [`SimStream`]s, dropping, delaying, duplicating and splitting them from a
//! seed, so a conversation replays.

mod errors;
mod types;
mod utils;

pub use crate::errors::bus_error::{BusError, BusErrorEnum};
pub use crate::types::buffer_pool::BufferPool;
pub use crate::types::connection::Connection;
pub use crate::types::network_faults::NetworkFaults;
pub use crate::types::network_stats::NetworkStats;
pub use crate::types::outbound::Outbound;
pub use crate::types::pooled_buffer::PooledBuffer;
pub use crate::types::received_frame::ReceivedFrame;
pub use crate::types::sim_network::SimNetwork;
pub use crate::types::sim_stream::SimStream;
