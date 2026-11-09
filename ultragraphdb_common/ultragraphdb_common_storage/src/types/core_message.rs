/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_common_role::Push;
use ultragraphdb_core_catalog::CatalogContents;

use crate::types::parked_frame::ParkedFrame;

/// Where every answer of a connection's requests goes, in the order the core gives them: each frame, or a stream's end with no
/// frame, and its stream. Its length is bounded by the connection: one push for each request it holds handed on, as
/// `storage.held_bytes` bounds them, and one for each stream it holds open.
pub type Pushes = UnboundedSender<(u64, Push)>;

/// What a connection hands its core's task.
#[derive(Debug)]
pub enum CoreMessage {
    /// A request of `connection` on `stream`, its frame copied out of the connection's receive buffer and counted among what the
    /// connection holds until the core drops it (`PROTOCOL.md` §11.12, *Parked requests*), answered through `pushes`.
    Request {
        /// The connection.
        connection: u64,
        /// The request's stream.
        stream: u64,
        /// The request's frame, its order on the connection checked.
        frame: ParkedFrame,
        /// Where its answer goes, and every frame answering it later.
        pushes: Pushes,
        /// The largest frame the connection's peer takes.
        max_frame: u32,
    },
    /// The peer of `connection` cancelled `stream`.
    Cancelled {
        /// The connection.
        connection: u64,
        /// The stream.
        stream: u64,
    },
    /// The frame last sent on `stream` of `connection` has left and the stream holds credit: there is room for its next.
    Ready {
        /// The connection.
        connection: u64,
        /// The stream.
        stream: u64,
    },
    /// The frame last sent on `stream` of `connection` has left, and the stream holds no credit for its next.
    Left {
        /// The connection.
        connection: u64,
        /// The stream.
        stream: u64,
    },
    /// A catalog version a close announced, whose round goes before that close is delivered (`PROTOCOL.md` §11.12).
    Catalog(CatalogContents),
    /// `connection` closed.
    Closed {
        /// The connection.
        connection: u64,
    },
}
