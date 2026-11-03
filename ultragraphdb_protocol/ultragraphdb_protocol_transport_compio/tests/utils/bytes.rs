/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whole writes and reads over any compio stream, for the tests.

use compio::BufResult;
use compio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Writes `bytes` whole to `stream` and flushes it.
pub async fn send<S: AsyncWrite>(stream: &mut S, bytes: &[u8]) -> TestResult {
    let BufResult(written, _) = stream.write_all(bytes.to_vec()).await;
    written.context("a write")?;
    stream.flush().await.context("a flush")?;
    Ok(())
}

/// Reads exactly `len` bytes from `stream`.
pub async fn receive<S: AsyncRead>(stream: &mut S, len: usize) -> TestResult<Vec<u8>> {
    let BufResult(read, bytes) = stream.read_exact(Vec::with_capacity(len)).await;
    read.context("a read")?;
    Ok(bytes)
}

/// What one read from `stream` gives: 0 bytes once its peer shut down.
pub async fn read_once<S: AsyncRead>(stream: &mut S) -> TestResult<usize> {
    let BufResult(read, _) = stream.read(Vec::with_capacity(8)).await;
    read.context("a read")
}
