/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Unix-socket addresses for the tests, their socket files inside the test's own temporary directory.
//!
//! A socket path holds at most 103 bytes on macOS and 107 on Linux (`sockaddr_un`). On macOS the root's `.bazelrc`
//! sets `test:macos --test_tmpdir=/tmp/ut`, so `TEST_TMPDIR` is `/private/tmp/ut/_tmp/<32 hex>`, 53 bytes, and a socket
//! named `<pid>-<n>-<name>` (at most 20 bytes for the names these tests use) stays far under the limit.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use ultragraphdb_protocol_transport::Address;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The longest socket path the platforms take: 104 bytes of `sun_path` on macOS, one of them the NUL.
const SOCKET_PATH_MAX: usize = 103;

/// The number of the next directory, so the tests of one process, run on several threads, never share a path.
// An atomic: the tests run on several threads at once. Relaxed suffices, since the counter orders nothing.
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A test's own names for its sockets in `TEST_TMPDIR`, or in the system's temporary directory outside Bazel.
pub struct SocketDir {
    dir: PathBuf,
    id: String,
}

impl SocketDir {
    /// Names apart from every other `SocketDir` of the process.
    pub fn new() -> TestResult<Self> {
        let dir = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let id = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Ok(Self { dir, id })
    }

    /// The `unix://` address of the socket `name`, a path checked to fit `sockaddr_un`.
    pub fn address(&self, name: &str) -> TestResult<Address> {
        let path = self.dir.join(format!("{}-{name}", self.id));
        let path = path.to_str().context("a UTF-8 path")?;
        if path.len() > SOCKET_PATH_MAX {
            return Err(TestError::new(format!(
                "{path} is {} bytes, past {SOCKET_PATH_MAX}: TEST_TMPDIR is too long for a socket",
                path.len()
            )));
        }
        Ok(Address::parse(&format!("unix://{path}"))?)
    }
}
