/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod begin_view_body_view;

use std::marker::PhantomData;

use ultragraphdb_core_ids::SessionToken;

use crate::types::read_level::ReadLevel;

/// The body of a received Begin frame, read in place; see [`crate::Begin`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginView<'a> {
    frame: PhantomData<&'a [u8]>,
    namespace: u64,
    read_version: u64,
    read_only: bool,
    level: ReadLevel,
    deadline_ms: u32,
    token: Option<SessionToken>,
}

impl<'a> BeginView<'a> {
    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The version the transaction's reads take; 0 where each partition gives it.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// Whether the transaction is read-only.
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The level of the transaction's reads.
    pub fn level(&self) -> ReadLevel {
        self.level
    }

    /// The deadline in milliseconds; 0 for the Welcome's `transaction_ms`.
    pub fn deadline_ms(&self) -> u32 {
        self.deadline_ms
    }

    /// The session token the client holds.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }
}
