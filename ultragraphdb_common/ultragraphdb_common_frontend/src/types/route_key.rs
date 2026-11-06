/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a stream on a role connection is to the session that opened it, each naming the client's stream it serves and the attempt
/// it belongs to, so what a stream abandoned sends late is told from the current one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteKey {
    /// A transaction's Bind stream.
    Bind {
        /// The transaction's stream.
        transaction: u64,
        /// Which of its Binds.
        attempt: u32,
    },
    /// A read's ComputeRead.
    Read {
        /// The read's stream.
        read: u64,
        /// Which of its sends.
        attempt: u32,
    },
    /// A one-frame write.
    OneFrame {
        /// The request's stream.
        stream: u64,
        /// Which of its sends.
        attempt: u32,
    },
    /// A Drop's Bind stream.
    Drop {
        /// The request's stream.
        stream: u64,
    },
    /// A Recall of a retried Commit.
    Recall {
        /// The request's stream.
        stream: u64,
        /// The partition asked.
        partition: u32,
    },
    /// The Reference learning a claimed value's predicate.
    Claim {
        /// The request's stream.
        stream: u64,
    },
}
