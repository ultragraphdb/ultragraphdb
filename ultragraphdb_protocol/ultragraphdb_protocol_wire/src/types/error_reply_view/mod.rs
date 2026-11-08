/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_view_body_view;

use crate::types::error_code::ErrorCode;
use crate::types::request_term_view::RequestTermView;
use crate::utils::constants::DETAIL_VALUE_CUT;

/// The body of a received Error frame, read in place; see [`crate::ErrorReply`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReplyView<'a> {
    code: ErrorCode,
    detail: u64,
    message: &'a str,
    value: Option<RequestTermView<'a>>,
}

impl<'a> ErrorReplyView<'a> {
    /// The error's code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The detail the code defines.
    pub fn detail(&self) -> u64 {
        self.detail
    }

    /// The message.
    pub fn message(&self) -> &'a str {
        self.message
    }

    /// The value a failed compare-and-set found, from the frame's one-entry term table; `None` when it holds none. Cut, or
    /// left out, when [`ErrorReplyView::value_cut`] says so.
    pub fn value(&self) -> Option<RequestTermView<'a>> {
        self.value
    }

    /// Whether this [`ErrorCode::OperationFailed`]'s value is cut to fit its frame, or left out when no cut keeps it
    /// whole: [`DETAIL_VALUE_CUT`] in its `detail` (`PROTOCOL.md` §9.4). The value is read again for it whole. Never for
    /// another code.
    #[must_use]
    pub fn value_cut(&self) -> bool {
        self.code == ErrorCode::OperationFailed && self.detail & DETAIL_VALUE_CUT != 0
    }
}
