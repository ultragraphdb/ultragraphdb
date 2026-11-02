/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A namespace, a tenant's part of the database. Its 8 big-endian bytes are hashed into the top 8 bits of every
/// bucket of its entities ([`Bucket::derive`](crate::Bucket::derive)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Namespace(u64);

impl Namespace {
    /// The namespace `namespace`.
    #[must_use]
    pub fn new(namespace: u64) -> Self {
        Self(namespace)
    }

    /// The namespace as a number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}
