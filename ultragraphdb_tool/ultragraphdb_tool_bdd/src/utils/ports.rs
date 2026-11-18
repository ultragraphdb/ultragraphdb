/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The ports a test target's processes listen at: a block of [`PORT_BLOCK`] ports from the base its `BUILD.bazel` sets in
//! [`PORT_BASE`], each port the base plus an offset the test declares. `bazel/test_ports.bzl` gives every integration target
//! a block of its own, so the targets run side by side, and each port stays fixed and known within its target.

use std::env;

use crate::types::test_error::{TestError, TestResult};

/// The variable naming the first port of the running target's block, set in its `BUILD.bazel` `env`.
pub const PORT_BASE: &str = "ULTRAGRAPHDB_TEST_PORT_BASE";

/// How many ports a target's block holds: every offset lies below it. `bazel/test_ports.bzl` spaces the bases this far
/// apart.
pub const PORT_BLOCK: u16 = 200;

/// The port `offset` above the running target's base.
///
/// # Errors
///
/// A [`TestError`] naming [`PORT_BASE`] when the variable is unset or not Unicode, and as [`port_from`] when its value or
/// `offset` is refused.
pub fn port(offset: u16) -> TestResult<u16> {
    let base = env::var(PORT_BASE).map_err(|error| {
        TestError::new(format!(
            "{PORT_BASE} is not set ({error}): the test target's BUILD.bazel sets it in `env`, from bazel/test_ports.bzl"
        ))
    })?;
    port_from(&base, offset)
}

/// The port `offset` above the base `base`.
///
/// # Errors
///
/// A [`TestError`] when `base` is no port, when `offset` is not below [`PORT_BLOCK`], or when the sum passes the last port.
pub fn port_from(base: &str, offset: u16) -> TestResult<u16> {
    let first: u16 = base
        .parse()
        .map_err(|error| TestError::new(format!("{PORT_BASE} {base:?} is no port ({error})")))?;
    if offset >= PORT_BLOCK {
        return Err(TestError::new(format!(
            "offset {offset} lies past a target's block of {PORT_BLOCK} ports"
        )));
    }
    first.checked_add(offset).ok_or_else(|| {
        TestError::new(format!(
            "{PORT_BASE} {first} plus {offset} passes the last port"
        ))
    })
}

/// The loopback address of the port `offset` above the running target's base, `127.0.0.1:<port>`.
///
/// # Errors
///
/// As [`port`].
pub fn loopback(offset: u16) -> TestResult<String> {
    Ok(format!("127.0.0.1:{}", port(offset)?))
}
