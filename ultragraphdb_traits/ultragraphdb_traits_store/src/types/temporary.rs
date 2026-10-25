/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A temporary keyspace of a [`crate::Store`], by its number: where a rebuild keeps what it builds until it is
/// swapped in, so the build's memory stays bounded. Opening the store drops every temporary keyspace, so a
/// `Temporary` names one only in the process that created it.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Temporary(u64);

impl Temporary {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}
