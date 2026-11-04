/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::parsed_head::HttpHead;

/// A whole request, owning the buffer it arrived in, so its body is taken with no copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    head: HttpHead,
    buffer: Vec<u8>,
}

impl HttpRequest {
    pub(crate) fn new(head: HttpHead, buffer: Vec<u8>) -> Self {
        Self { head, buffer }
    }

    /// The method.
    #[must_use]
    pub fn method(&self) -> &str {
        self.head.method()
    }

    /// The target, its path and its query.
    #[must_use]
    pub fn target(&self) -> &str {
        self.head.target()
    }

    /// The target's path, without its query.
    #[must_use]
    pub fn path(&self) -> &str {
        self.target().split('?').next().unwrap_or_default()
    }

    /// The value of the query's parameter `name`.
    #[must_use]
    pub fn query(&self, name: &str) -> Option<&str> {
        self.target()
            .split_once('?')?
            .1
            .split('&')
            .find_map(|pair| pair.split_once('=').filter(|(key, _)| *key == name))
            .map(|(_, value)| value)
    }

    /// The key the request presents.
    #[must_use]
    pub fn bearer(&self) -> Option<&str> {
        self.head.bearer()
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        let start = self.head.body_start;
        self.buffer
            .get(start..start + self.head.body_len)
            .unwrap_or_default()
    }

    /// The body, the buffer given up.
    #[must_use]
    pub fn into_body(mut self) -> Vec<u8> {
        self.buffer
            .truncate(self.head.body_start + self.head.body_len);
        let _head: Vec<u8> = self.buffer.drain(..self.head.body_start).collect();
        self.buffer
    }
}
