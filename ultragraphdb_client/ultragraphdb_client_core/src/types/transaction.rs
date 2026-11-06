/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ReadLevel;

/// A transaction begun on a stream of the client's connection: its tenant, the snapshot it reads, the level its reads
/// share and whether it is read-only. Every request of it goes on its stream; it ends with a commit, an abort, or a refusal on its stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transaction {
    stream: u64,
    generation: u64,
    tenant: u64,
    read_version: u64,
    level: ReadLevel,
    read_only: bool,
}

impl Transaction {
    pub(crate) fn new(
        stream: u64,
        generation: u64,
        tenant: u64,
        read_version: u64,
        level: ReadLevel,
        read_only: bool,
    ) -> Self {
        Self {
            stream,
            generation,
            tenant,
            read_version,
            level,
            read_only,
        }
    }

    /// The stream the transaction runs on.
    #[must_use]
    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The tenant the transaction reads and writes.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version of the snapshot it reads.
    #[must_use]
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The level its reads share (`PROTOCOL.md` §11.11).
    #[must_use]
    pub fn level(&self) -> ReadLevel {
        self.level
    }

    /// Whether the transaction is read-only.
    #[must_use]
    pub fn read_only(&self) -> bool {
        self.read_only
    }
}
