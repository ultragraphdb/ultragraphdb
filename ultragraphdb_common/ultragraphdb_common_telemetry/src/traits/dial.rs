/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Debug;
use std::io::{self, Read, Write};
use std::time::Duration;

/// Opens the connection one request of [`OtlpHttpClient`](crate::OtlpHttpClient) runs over: [`TcpDial`](crate::TcpDial)
/// in production, an in-memory stand-in for a collector in tests.
pub trait Dial: Debug + Send + Sync {
    /// The connection a dial opens.
    type Stream: Read + Write;

    /// A connection to `host` at `port`, opened within `timeout`, whose every read and write waits at most
    /// `timeout`.
    ///
    /// # Errors
    ///
    /// The I/O error of a name that resolves to nothing, or of the last address that did not accept.
    fn dial(&self, host: &str, port: u16, timeout: Duration) -> io::Result<Self::Stream>;
}
