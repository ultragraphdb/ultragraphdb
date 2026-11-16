/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A request's head: its method, its target, the key it presents, and where its body lies in the buffer. Its `Debug`
/// says whether a key is presented, never the key.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpHead {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) bearer: Option<String>,
    pub(crate) body_start: usize,
    pub(crate) body_len: usize,
}

impl std::fmt::Debug for HttpHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpHead")
            .field("method", &self.method)
            .field("target", &self.target)
            .field(
                "bearer",
                &self.bearer.as_ref().map(|_| format_args!("<redacted>")),
            )
            .field("body_start", &self.body_start)
            .field("body_len", &self.body_len)
            .finish()
    }
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
