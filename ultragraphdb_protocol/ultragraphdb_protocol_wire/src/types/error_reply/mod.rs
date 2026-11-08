/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::error_code::ErrorCode;
use crate::types::request_term::RequestTerm;
use crate::utils::constants::{DETAIL_TRANSACTION_NOT_HELD, DETAIL_VALUE_CUT};

/// The Error frame: a request refused on the frame's stream, or, on stream 0, the connection's last frame after a
/// connection error, a refused handshake or authentication, or an expired credential. Its code, a detail the code
/// defines, a message, and for a failed compare-and-set the value found. Its frame fits the floor of `max_frame`,
/// [`crate::MAX_FRAME_FLOOR`], which every peer takes: the value takes what that frame leaves beside an empty message, a
/// literal's lexical form past it cut at its last character boundary within it and a term no cut keeps whole left out,
/// either flagged by [`crate::DETAIL_VALUE_CUT`] in `detail`; the message takes what the value leaves, cut the same way.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `code` | u16, an [`ErrorCode`] |
/// | 8 | `detail` | u64; for [`ErrorCode::OperationFailed`], bit 0 [`crate::DETAIL_VALUE_CUT`], every other bit 0 |
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
    /// A reply of `code` saying `message`, its detail zero.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            detail: 0,
            message: message.into(),
            value: None,
        }
    }

    /// The error's code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The detail the code defines, as the reply was given it: its frame adds [`DETAIL_VALUE_CUT`] when it cuts the
    /// value.
    pub fn detail(&self) -> u64 {
        self.detail
    }

    /// The message, whole: its frame carries it cut to fit when it passes what the frame leaves it.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The value a failed compare-and-set found; `None` when the predicate held none, or when a frame that cut it left it
    /// out. Whole as the reply was given it: its frame carries it cut to fit when it passes what the frame leaves it.
    pub fn value(&self) -> Option<&RequestTerm> {
        self.value.as_ref()
    }

    /// Whether the frame of this [`ErrorCode::OperationFailed`] carries its value cut, or leaves it out, which
    /// [`DETAIL_VALUE_CUT`] in its `detail` says: the reply's own `detail` says so, read from a frame that cut it, or the
    /// value passes what the frame leaves it (`PROTOCOL.md` §9.4). Never for another code.
    #[must_use]
    pub fn value_cut(&self) -> bool {
        self.code == ErrorCode::OperationFailed
            && (self.detail & DETAIL_VALUE_CUT != 0
                || self.value.as_ref().is_some_and(|value| {
                    error_reply_body_builder::text_length(value)
                        > error_reply_body_builder::value_room()
                }))
    }

    /// The reply to `error`: [`ErrorCode::Protocol`] with the error's text.
    pub fn protocol(error: &ProtocolError) -> Self {
        Self::new(ErrorCode::Protocol, error.to_string())
    }

    /// The reply to a Scan naming a transaction the partition does not hold: [`ErrorCode::Protocol`] with `detail`
    /// [`crate::DETAIL_TRANSACTION_NOT_HELD`], saying `message`, which compute relays with that `detail`.
    pub fn transaction_not_held(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Protocol, message).with_detail(DETAIL_TRANSACTION_NOT_HELD)
    }

    /// The reply with the `detail` its code defines.
    pub fn with_detail(mut self, detail: u64) -> Self {
        self.detail = detail;
        self
    }

    /// The reply carrying the `value` a failed compare-and-set found: a term one entry of a term table holds, so not a
    /// triple term, and neither a literal stating `xsd:string` nor one with an empty language tag, which a table
    /// refuses. Its frame cuts it to fit, as [`ErrorReply::value_cut`] says.
    pub fn with_value(mut self, value: RequestTerm) -> Self {
        self.value = Some(value);
        self
    }

    /// Whether the client retries: [`ErrorCode::retryable`].
    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }
}
