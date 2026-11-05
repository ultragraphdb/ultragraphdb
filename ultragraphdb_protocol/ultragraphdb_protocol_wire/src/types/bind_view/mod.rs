/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod bind_view_body_view;

use std::marker::PhantomData;

use crate::types::bind_kind::BindKind;

/// The body of a received Bind frame, read in place; see [`crate::Bind`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindView<'a> {
    frame: PhantomData<&'a [u8]>,
    partition: u32,
    kind: BindKind,
    tenant: u64,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
}

impl BindView<'_> {
    /// The partition bound to.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// What is bound.
    pub fn kind(&self) -> BindKind {
        self.kind
    }

    /// The transaction's tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version the transaction reads at; 0 as sent and for a Drop.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The client's token's epoch; 0 for none.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The transaction's identity.
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }
}
