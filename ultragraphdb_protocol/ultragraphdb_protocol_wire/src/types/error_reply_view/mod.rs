/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod error_reply_view_body_view;

use crate::types::error_code::ErrorCode;

/// The body of a received Error frame, read in place; see [`crate::ErrorReply`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorReplyView<'a> {
    code: ErrorCode,
    detail: u64,
    message: &'a str,
    value: Option<&'a str>,
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

    pub fn value(&self) -> Option<&'a str> {
        self.value
    }
}
