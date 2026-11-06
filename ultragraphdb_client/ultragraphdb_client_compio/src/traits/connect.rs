/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::io;

use crate::traits::split_stream::SplitStream;

/// How the client opens a connection: a new compio stream to the server each time it is asked.
pub trait Connect {
    /// The stream a connection is read and written through, split into its two halves once open.
    type Stream: SplitStream;

    /// A new stream to the server.
    fn connect(&mut self) -> impl Future<Output = io::Result<Self::Stream>>;
}
