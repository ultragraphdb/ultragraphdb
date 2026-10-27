/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_body_builder;

use crate::types::error_code::ErrorCode;
use ultragraphdb_error_protocol::ProtocolError;

/// The Error frame: a request refused on the frame's stream, or, on stream 0, the connection's last frame after a
/// protocol error. Its code, a detail the code defines, a message, and for a failed compare-and-set the value found.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `code` | u16, an [`ErrorCode`] |
/// | 8 | `detail` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the message, UTF-8 |
/// | 1 | the value found, an N-Triples term, or empty |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReply {
    code: ErrorCode,
    detail: u64,
    message: String,
    value: Option<String>,
}

impl ErrorReply {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            detail: 0,
            message: message.into(),
            value: None,
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn detail(&self) -> u64 {
        self.detail
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// The reply to `error`: [`ErrorCode::Protocol`] with the error's text.
    pub fn protocol(error: &ProtocolError) -> Self {
        Self::new(ErrorCode::Protocol, error.to_string())
    }

    pub fn with_detail(mut self, detail: u64) -> Self {
        self.detail = detail;
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Whether the client retries: [`ErrorCode::retryable`].
    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }
}
