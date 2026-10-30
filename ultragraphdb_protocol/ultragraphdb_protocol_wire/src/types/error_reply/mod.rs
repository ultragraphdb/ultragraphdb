/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::error_code::ErrorCode;
use crate::types::request_term::RequestTerm;

/// The Error frame: a request refused on the frame's stream, or, on stream 0, the connection's last frame after a
/// connection error, a refused handshake or authentication, or an expired credential. Its code, a detail the code
/// defines, a message, and for a failed compare-and-set the value found.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `code` | u16, an [`ErrorCode`] |
/// | 8 | `detail` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the message, UTF-8 |
/// | 1 | the value found: a request term table's records, one entry; empty for none |
/// | 2 | that term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReply {
    code: ErrorCode,
    detail: u64,
    message: String,
    value: Option<RequestTerm>,
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

    /// The value a failed compare-and-set found; `None` when the predicate held none.
    pub fn value(&self) -> Option<&RequestTerm> {
        self.value.as_ref()
    }

    /// The reply to `error`: [`ErrorCode::Protocol`] with the error's text.
    pub fn protocol(error: &ProtocolError) -> Self {
        Self::new(ErrorCode::Protocol, error.to_string())
    }

    pub fn with_detail(mut self, detail: u64) -> Self {
        self.detail = detail;
        self
    }

    /// The reply carrying the `value` a failed compare-and-set found: a term one entry of a term table holds, so not a
    /// triple term, and neither a literal stating `xsd:string` nor one with an empty language tag, which a table
    /// refuses.
    pub fn with_value(mut self, value: RequestTerm) -> Self {
        self.value = Some(value);
        self
    }

    /// Whether the client retries: [`ErrorCode::retryable`].
    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }
}
