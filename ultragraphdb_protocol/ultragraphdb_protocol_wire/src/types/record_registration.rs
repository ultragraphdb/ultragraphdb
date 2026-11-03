/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, SystemId};

use crate::utils::constants::RECORD_REGISTRATION_SIZE;

/// A registration in a write-set record's section 4 (`PROTOCOL.md` §12.2), 32 bytes: `entity`, its system ID, 16 bytes
/// big-endian; `group` u32, a bucket group; 4 zero bytes; `generation` u64, at least 1. In a transaction's record it
/// is a registration the transaction relied on as a referrer, the generation the home's reply named; in a home's
/// record (kind 2), the referrer-set member it adds or renews, the member's generation it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordRegistration {
    entity: SystemId,
    group: BucketGroup,
    generation: NonZeroU64,
}

impl RecordRegistration {
    /// The registration of `entity` for the bucket group `group` at `generation`.
    pub fn new(entity: SystemId, group: BucketGroup, generation: NonZeroU64) -> Self {
        Self {
            entity,
            group,
            generation,
        }
    }

    /// The entity's system ID.
    pub fn entity(&self) -> SystemId {
        self.entity
    }

    /// The bucket group.
    pub fn group(&self) -> BucketGroup {
        self.group
    }

    /// The generation, at least 1.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// The registration's 32 bytes.
    pub fn encode(&self) -> [u8; RECORD_REGISTRATION_SIZE] {
        let fields = self
            .entity
            .to_be_bytes()
            .into_iter()
            .chain(self.group.get().to_le_bytes())
            .chain([0; 4])
            .chain(self.generation.get().to_le_bytes());
        let mut out = [0u8; RECORD_REGISTRATION_SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The registration `bytes` hold: `None` for an invalid entity, a group of 2¹⁸ or more, nonzero bytes 20 to 23 or a
    /// generation of 0.
    pub(crate) fn decode(bytes: &[u8; RECORD_REGISTRATION_SIZE]) -> Option<Self> {
        let (entity, rest) = bytes.split_first_chunk::<16>()?;
        let (group, rest) = rest.split_first_chunk::<4>()?;
        let (zero, generation) = rest.split_first_chunk::<4>()?;
        if *zero != [0; 4] {
            return None;
        }
        Some(Self {
            entity: SystemId::decode(entity).ok()?,
            group: BucketGroup::new(u32::from_le_bytes(*group)).ok()?,
            generation: NonZeroU64::new(u64::from_le_bytes(<[u8; 8]>::try_from(generation).ok()?))?,
        })
    }
}
