/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::UnboundedSender;
use futures::channel::oneshot;
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_transport::ReceivedFrame;

/// Where a connection's unasked frames go: each frame and its stream.
pub type Pushes = UnboundedSender<(u64, Vec<u8>)>;

/// What a connection hands its core's task.
#[derive(Debug)]
pub enum CoreMessage {
    /// A request of `connection` on `stream`, its frame whole, answered on `done`.
    Request {
        /// The connection.
        connection: u64,
        /// The request's stream.
        stream: u64,
        /// The request's frame.
        frame: ReceivedFrame,
        /// Where its answer goes.
        done: oneshot::Sender<Answer>,
        /// Where the frames answering it later go.
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
    /// `connection` closed.
    Closed {
        /// The connection.
        connection: u64,
    },
}
