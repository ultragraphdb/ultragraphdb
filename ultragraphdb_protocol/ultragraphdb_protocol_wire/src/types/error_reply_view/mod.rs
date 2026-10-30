/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_view_body_view;

use crate::types::error_code::ErrorCode;
use crate::types::request_term_view::RequestTermView;

/// The body of a received Error frame, read in place; see [`crate::ErrorReply`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorReplyView<'a> {
    code: ErrorCode,
    detail: u64,
    message: &'a str,
    value: Option<RequestTermView<'a>>,
}

impl<'a> ErrorReplyView<'a> {
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn detail(&self) -> u64 {
        self.detail
    }

    pub fn message(&self) -> &'a str {
        self.message
    }

    /// The value a failed compare-and-set found, from the frame's one-entry term table; `None` when it holds none.
    pub fn value(&self) -> Option<RequestTermView<'a>> {
        self.value
    }
}
