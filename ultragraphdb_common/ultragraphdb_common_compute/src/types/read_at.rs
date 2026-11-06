/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_wire::{ReadLevel, ScanAt};

use crate::types::read_request::ReadRequest;

/// How a read's first Scan of a partition chooses the version it reads (`PROTOCOL.md` §13.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadAt {
    pub(crate) at: ScanAt,
    pub(crate) read_version: u64,
    pub(crate) epoch: u64,
}

impl ReadAt {
    /// At `epoch`'s version bound, once the partition has closed it: a monotonic-fast read reaching several partitions, and a
    /// reverse read's members.
    pub(crate) fn at_epoch(epoch: u64) -> Self {
        let bound = Epoch::new(epoch).map_or(0, Version::bound_of);
        Self {
            at: ScanAt::ReadVersion,
            read_version: bound,
            epoch,
        }
    }

    /// At the version a Scan before this one chose.
    pub(crate) fn chosen(version: u64) -> Self {
        Self {
            at: ScanAt::ReadVersion,
            read_version: version,
            epoch: 0,
        }
    }

    /// At the partition's latest applied version, waiting for nothing: where a referrer set holds every member a named version's
    /// epoch may name, since a set only grows.
    pub(crate) fn latest() -> Self {
        Self {
            at: ScanAt::Latest,
            read_version: 0,
            epoch: 0,
        }
    }

    /// How `read` first reads a partition, before any answer named an epoch.
    pub(crate) fn of(read: &ReadRequest) -> Self {
        if !read.read_only || read.named {
            let at = if read.named {
                ScanAt::Named
            } else {
                ScanAt::ReadVersion
            };
            return Self {
                at,
                read_version: read.read_version,
                epoch: 0,
            };
        }
        match read.level {
            ReadLevel::Snapshot => Self {
                at: ScanAt::ReadVersion,
                read_version: read.read_version,
                epoch: read.epoch,
            },
            ReadLevel::Latest => Self {
                at: ScanAt::Latest,
                read_version: 0,
                epoch: read.epoch,
            },
            ReadLevel::MonotonicFast => Self {
                at: ScanAt::LatestClosed,
                read_version: 0,
                epoch: read.epoch,
            },
        }
    }
}
