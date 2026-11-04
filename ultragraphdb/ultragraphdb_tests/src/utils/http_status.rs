/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use ultragraphdb_tool_bdd::{Context, TestResult};

/// How long one probe may take, so a listener that accepts and says nothing fails the test.
const PROBE_TIME: Duration = Duration::from_secs(2);

/// The status `address` answers a `GET` of `path` with.
///
/// # Errors
///
/// The connection refused, or an answer that is no HTTP response.
pub fn http_status(address: &str, path: &str) -> TestResult<u16> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(PROBE_TIME))?;
    stream.set_write_timeout(Some(PROBE_TIME))?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {address}\r\n\r\n")?;
    let mut answer = String::new();
    let _read: usize = stream.read_to_string(&mut answer)?;
    let status = answer.split(' ').nth(1).context("a status")?;
    Ok(status.parse()?)
}
