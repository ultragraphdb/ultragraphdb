/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

/// How the client opens a connection: a new byte stream to the server each time it is asked.
pub trait Connect {
    type Stream: AsyncRead + AsyncWrite + Unpin;

    /// A new stream to the server.
    fn connect(&mut self) -> impl Future<Output = io::Result<Self::Stream>>;
}
