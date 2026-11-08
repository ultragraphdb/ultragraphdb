/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod bind_body_builder;

use crate::types::bind_kind::BindKind;

/// The frontend binds a read-write transaction, or a Drop, to a partition (`PROTOCOL.md` §13.10): the frame's stream,
/// on the connection to the storage process hosting the partition, becomes the transaction's there and stays open.
/// Sent with `read_version` 0; answered with every field as sent and, for a transaction, `read_version` the
/// partition's visibility watermark when it binds.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32, at least 1 |
/// | 4 | `kind` | u8, a [`BindKind`] |
/// | 8 | `tenant` | u64 |
/// | 16 | `read_version` | u64: 0 as sent; as answered, the watermark for a transaction, 0 for a Drop |
/// | 24 | `epoch` | u64: the client's token's epoch, 0 for none |
/// | 32 | `principal` | u64: the client's principal |
/// | 40 | `transaction` | 16 bytes: the transaction's identity, not all zero |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bind {
    partition: u32,
    kind: BindKind,
    tenant: u64,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<Bind>() <= 64);

impl Bind {
    /// The binding of `transaction`, of `kind`, in `tenant`, acting for `principal`, to `partition`, committing at
    /// `epoch` or later (0 for none).
    #[must_use]
    pub fn new(
        partition: u32,
        kind: BindKind,
        tenant: u64,
        epoch: u64,
        principal: u64,
        transaction: [u8; 16],
    ) -> Self {
        Self {
            partition,
            kind,
            tenant,
            read_version: 0,
            epoch,
            principal,
            transaction,
        }
    }

    /// The reply: the version the transaction reads at, the partition's visibility watermark.
    #[must_use]
    pub fn with_read_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self
    }

    /// The partition bound to.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// What is bound.
    #[must_use]
    pub fn kind(&self) -> BindKind {
        self.kind
    }

    /// The transaction's tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version the transaction reads at; 0 as sent and for a Drop.
    #[must_use]
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The client's token's epoch; 0 for none.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The transaction's identity.
    #[must_use]
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }
}
