/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod begin_body_builder;

/// A transaction begins on the frame's stream: sent with `read_version` zero, answered with the snapshot it reads.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `read_version` | u64 |
/// | 16 | `mode` | u8: 0 read-write, 1 read-only |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Begin {
    namespace: u64,
    read_version: u64,
    read_only: bool,
}

impl Begin {
    pub fn new(namespace: u64, read_version: u64, read_only: bool) -> Self {
        Self {
            namespace,
            read_version,
            read_only,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }
}
