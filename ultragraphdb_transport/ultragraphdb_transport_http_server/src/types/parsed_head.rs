/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A request's head: its method, its target, the key it presents, and where its body lies in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHead {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) bearer: Option<String>,
    pub(crate) body_start: usize,
    pub(crate) body_len: usize,
}

impl HttpHead {
    /// The method.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The target, its path and its query.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The key an `Authorization: Bearer` header presents.
    #[must_use]
    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_deref()
    }

    /// The body's length.
    #[must_use]
    pub fn body_len(&self) -> usize {
        self.body_len
    }
}

/// What the bytes received on a connection ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedHead {
    /// They end no request, and more may.
    Incomplete,
    /// A whole request, the buffer holding its body.
    Complete(HttpHead),
    /// Refused with this status, without reading further.
    Refused(u16),
}
