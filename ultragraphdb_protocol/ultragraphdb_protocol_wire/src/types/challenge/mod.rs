/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod challenge_body_builder;

/// The first half of verifying a principal's SCRAM-SHA-256 proof (`PROTOCOL.md` §13.8): a frontend asks the metadata
/// group's host for the principal's salt and iteration count. Sent with `iterations` 0 and no salt, answered with both.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `iterations` | u32: 0 in a request; 4,096 to 10,000,000 in a reply |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the principal's name, UTF-8, prepared with SASLprep |
/// | 1 | the salt; empty in a request, not empty in a reply |
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Challenge {
    name: String,
    iterations: u32,
    salt: Vec<u8>,
}

impl Challenge {
    /// The request for the principal `name`'s salt and count.
    pub fn request(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            iterations: 0,
            salt: Vec::new(),
        }
    }

    /// The reply: the principal's `iterations` and `salt`.
    #[must_use]
    pub fn with_salt(mut self, iterations: u32, salt: Vec<u8>) -> Self {
        self.iterations = iterations;
        self.salt = salt;
        self
    }

    /// The principal's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The iteration count; 0 in a request.
    #[must_use]
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// The salt; empty in a request.
    #[must_use]
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
}
