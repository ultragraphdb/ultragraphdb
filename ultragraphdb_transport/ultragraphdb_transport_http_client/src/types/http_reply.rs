/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A whole reply: its status, its headers and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl HttpReply {
    /// The status.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The value of the header `name`, its case ignored.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(held, _)| held.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The body, the reply given up.
    #[must_use]
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }
}

/// What the bytes received of a reply come to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedReply {
    /// They end no reply, and more may.
    Incomplete,
    /// A whole reply.
    Complete(HttpReply),
    /// They are no reply, for this reason.
    Malformed(String),
}
