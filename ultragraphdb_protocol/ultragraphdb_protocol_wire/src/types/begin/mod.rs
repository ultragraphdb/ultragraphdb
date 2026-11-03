/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod begin_body_builder;

use ultragraphdb_core_ids::SessionToken;

use crate::types::read_level::ReadLevel;

/// A transaction begins on the frame's stream (`PROTOCOL.md` §6.2), naming what every read of it shares: its level,
/// the session token its client holds, its deadline and the read version the client names, if any. Answered with
/// every field as sent and `read_version` the version its reads take.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `read_version` | u64: 0, or the version a read-only transaction at level 0 names |
/// | 16 | `mode` | u8: 0 read-write, 1 read-only |
/// | 17 | `level` | u8, a [`ReadLevel`] |
/// | 20 | `deadline_ms` | u32: milliseconds from the Begin's arrival; 0 for the Welcome's `transaction_ms` |
/// | 24 | `token` | session token, 24 bytes; zero for none |
///
/// No sections.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Begin {
    namespace: u64,
    read_version: u64,
    read_only: bool,
    level: ReadLevel,
    deadline_ms: u32,
    token: Option<SessionToken>,
}

impl Begin {
    /// A transaction in `namespace` reading at `read_version`, read-only or read-write, at level snapshot, with the
    /// Welcome's deadline and no token.
    pub fn new(namespace: u64, read_version: u64, read_only: bool) -> Self {
        Self {
            namespace,
            read_version,
            read_only,
            level: ReadLevel::Snapshot,
            deadline_ms: 0,
            token: None,
        }
    }

    /// The transaction at `level`.
    pub fn with_level(mut self, level: ReadLevel) -> Self {
        self.level = level;
        self
    }

    /// The transaction's deadline, `deadline_ms` from the Begin's arrival; 0 for the Welcome's `transaction_ms`.
    pub fn with_deadline_ms(mut self, deadline_ms: u32) -> Self {
        self.deadline_ms = deadline_ms;
        self
    }

    /// The transaction carrying the session token `token`.
    pub fn with_token(mut self, token: SessionToken) -> Self {
        self.token = Some(token);
        self
    }

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
