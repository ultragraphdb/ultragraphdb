/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[storage]` table: a storage process and the partitions it claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageTable {
    partition_first: Option<u32>,
    partition_count: Option<u32>,
    role_address: Option<String>,
    data_dir: Option<String>,
    close_request_ms: Option<u64>,
    scan_stall_ms: Option<u64>,
    read_retain_ms: Option<u64>,
    held_bytes: Option<u64>,
}

impl StorageTable {
    /// `read_retain_ms` when the table names none: 60,000 ms (`PROTOCOL.md` §11.9).
    pub const READ_RETAIN_MS_DEFAULT: u64 = 60_000;

    /// `held_bytes` when the table names none: 4,194,304 bytes (`PROTOCOL.md` §11.9).
    pub const HELD_BYTES_DEFAULT: u64 = 4_194_304;

    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            partition_first: reader.u32("storage", "partition_first")?,
            partition_count: reader.u32("storage", "partition_count")?,
            role_address: reader.string("storage", "role_address")?,
            data_dir: reader.string("storage", "data_dir")?,
            close_request_ms: reader.u64("storage", "close_request_ms")?,
            scan_stall_ms: reader.u64("storage", "scan_stall_ms")?,
            read_retain_ms: reader.u64("storage", "read_retain_ms")?,
            held_bytes: reader.u64("storage", "held_bytes")?,
        })
    }

    /// `partition_first`: the first partition it claims.
    #[must_use]
    pub fn partition_first(&self) -> Option<u32> {
        self.partition_first
    }

    /// `partition_count`: how many partitions it claims from the first.
    #[must_use]
    pub fn partition_count(&self) -> Option<u32> {
        self.partition_count
    }

    /// `role_address`: the address its partitions' role addresses derive from.
    #[must_use]
    pub fn role_address(&self) -> Option<&str> {
        self.role_address.as_deref()
    }

    /// `data_dir`: the directory of its partitions' logs and stores.
    #[must_use]
    pub fn data_dir(&self) -> Option<&str> {
        self.data_dir.as_deref()
    }

    /// `close_request_ms`: how long a close request goes unanswered before the partition answers `Busy`, in milliseconds.
    #[must_use]
    pub fn close_request_ms(&self) -> Option<u64> {
        self.close_request_ms
    }

    /// `scan_stall_ms`: how long a partition keeps a Scan with no frame able to leave, in milliseconds.
    #[must_use]
    pub fn scan_stall_ms(&self) -> Option<u64> {
        self.scan_stall_ms
    }

    /// `read_retain_ms`: how long a closed epoch stays readable between a read-only transaction's Scans, in
    /// milliseconds.
    #[must_use]
    pub fn read_retain_ms(&self) -> Option<u64> {
        self.read_retain_ms
    }

    /// `held_bytes`: the most bytes of frames a connection holds parked before the partition stops reading it.
    #[must_use]
    pub fn held_bytes(&self) -> Option<u64> {
        self.held_bytes
    }
}
