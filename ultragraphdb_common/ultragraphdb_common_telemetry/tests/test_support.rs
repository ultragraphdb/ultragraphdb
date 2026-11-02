/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The writer and the span exporter the unit tests and the scenarios install the subscriber over.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SpanData, SpanExporter};

/// What the fmt layer writes, shared with the test.
#[derive(Clone, Default)]
pub struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    pub fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8(bytes.clone()).expect("the fmt layer writes UTF-8")
    }
}

impl io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The in-memory exporter, kept through the shutdown the guard's drop sends it: the SDK's own exporter clears its
/// spans on shutdown.
#[derive(Debug, Clone, Default)]
pub struct Kept(pub InMemorySpanExporter);

impl SpanExporter for Kept {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        self.0.export(batch).await
    }
}
