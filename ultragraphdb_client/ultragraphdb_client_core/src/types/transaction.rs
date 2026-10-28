/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A transaction begun on a stream of the client's connection: its namespace, the snapshot it reads and whether it
/// is read-only. Every request of it goes on its stream; it ends with a commit, an abort, or a refusal on its stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transaction {
    stream: u64,
    generation: u64,
    namespace: u64,
    read_version: u64,
    read_only: bool,
}

impl Transaction {
    pub(crate) fn new(
        stream: u64,
        generation: u64,
        namespace: u64,
        read_version: u64,
        read_only: bool,
    ) -> Self {
        Self {
            stream,
            generation,
            namespace,
            read_version,
            read_only,
        }
    }

    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The version of the snapshot it reads.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }
}
