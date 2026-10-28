/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transport that carries the wire protocol, sans-I/O: pure, synchronous state machines with no runtime, which a
//! shell for a runtime drives (`ultragraphdb_protocol_transport_compio` for compio).
//!
//! A [`Framer`] assembles frames pull-style: it tells the shell how many bytes the frame's [`PooledBuffer`], taken
//! from the core's [`BufferPool`], must hold before it looks again, first the 256-byte header, verified for its
//! cluster before its `size` is trusted, then the rest of the frame into the same buffer, so bytes land where the
//! frame stays; the [`ReceivedFrame`] is viewed in place. [`ClientHandshake`] and [`ServerHandshake`] decide Hello
//! and Welcome: the protocol chosen, the limits, the refusals. [`Outbound`] schedules a server's frames across a
//! connection's streams: replies go out as they are pushed, Result frames only while their stream holds credit, and a
//! cancelled stream sends nothing more. The [`SimNetwork`] carries messages between [`SimStream`]s, dropping,
//! delaying, duplicating and splitting them from a seed, so a conversation replays.

mod errors;
mod types;
mod utils;

pub use crate::errors::transport_error::{TransportError, TransportErrorEnum};
pub use crate::types::buffer_pool::BufferPool;
pub use crate::types::client_handshake::ClientHandshake;
pub use crate::types::frame_step::FrameStep;
pub use crate::types::framer::Framer;
pub use crate::types::handshake_answer::HandshakeAnswer;
pub use crate::types::network_faults::NetworkFaults;
pub use crate::types::network_stats::NetworkStats;
pub use crate::types::outbound::Outbound;
pub use crate::types::pooled_buffer::PooledBuffer;
pub use crate::types::received_frame::ReceivedFrame;
pub use crate::types::server_handshake::ServerHandshake;
pub use crate::types::sim_network::SimNetwork;
pub use crate::types::sim_stream::SimStream;
