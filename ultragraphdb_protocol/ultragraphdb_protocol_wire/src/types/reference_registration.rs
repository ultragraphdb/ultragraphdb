/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{BucketGroup, SystemId};

use crate::types::reference_status::ReferenceStatus;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;

/// The size of a Reference body's registration.
pub(crate) const REGISTRATION_SIZE: usize = 48;

/// A registration of a Reference body (`PROTOCOL.md` §13.15), 48 bytes: `partition` u32, the entity's home;
/// `group` u32, the referring subject's bucket group, `ABSENT` for none; `iri` u32, the entity's IRI as an index
/// into the body's term table, `ABSENT` when `entity` names it; `outcome` u16, 0 accepted or the error code refusing
/// it; `status` u8, in an accepted get-only reply what the home found, else 0; 1 zero byte; `generation` u64; `close`
/// u64, an epoch; `entity` 16 bytes, a system ID big-endian or all zero.
///
/// A request names the entity by its IRI or its ID, the generation last seen and no outcome or close; the home's
/// reply keeps every field as sent and fills `outcome`, and, for an accepted registration, the entity's ID, the
/// member's generation and the latest close delivered to the home.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReferenceRegistration {
    partition: u32,
    group: Option<BucketGroup>,
    iri: Option<u32>,
    outcome: u16,
    status: u8,
    generation: u64,
    close: u64,
    entity: Option<SystemId>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ReferenceRegistration>() <= 64);

impl ReferenceRegistration {
    /// A request's registration at the home `partition`, for the bucket group `group` (`None` for a predicate, a
    /// graph or the frontend), naming the entity by the term-table index `iri` or by `entity`, with the
    /// `generation` last seen, 0 for none.
    #[must_use]
    pub fn request(
        partition: u32,
        group: Option<BucketGroup>,
        iri: Option<u32>,
        entity: Option<SystemId>,
        generation: u64,
    ) -> Self {
        Self {
            partition,
            group,
            iri,
            outcome: 0,
            status: 0,
            generation,
            close: 0,
            entity,
        }
    }

    /// The registration accepted: the entity's ID, the member's `generation` and the latest `close` delivered to the
    /// home.
    #[must_use]
    pub fn accepted(self, entity: SystemId, generation: u64, close: u64) -> Self {
        Self {
            outcome: 0,
            status: 0,
            generation,
            close,
            entity: Some(entity),
            ..self
        }
    }

    /// The registration of a get-only batch accepted with `status`: the entity's ID for a live, deleted or retracted one,
    /// `None` for an absent one, the member's `generation` and the latest `close` delivered to the home.
    #[must_use]
    pub fn answered(
        self,
        status: ReferenceStatus,
        entity: Option<SystemId>,
        generation: u64,
        close: u64,
    ) -> Self {
        Self {
            outcome: 0,
            status: status.number(),
            generation,
            close,
            entity,
            ..self
        }
    }

    /// The registration refused with the error code `outcome`: 26 (WrongPartition), 7 (Refused), 17 (Unavailable) or
    /// 30 (EntityDeleted); its status, generation and close 0, its entity as sent.
    #[must_use]
    pub fn refused(self, outcome: u16) -> Self {
        Self {
            outcome,
            status: 0,
            generation: 0,
            close: 0,
            entity: self.entity.filter(|_| self.iri.is_none()),
            ..self
        }
    }

    /// The entity's home partition.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The referring subject's bucket group; `None` for none.
    #[must_use]
    pub fn group(&self) -> Option<BucketGroup> {
        self.group
    }

    /// The entity's IRI as a term-table index; `None` when `entity` names it.
    #[must_use]
    pub fn iri(&self) -> Option<u32> {
        self.iri
    }

    /// 0 for an accepted registration and in a request; else the error code refusing it.
    #[must_use]
    pub fn outcome(&self) -> u16 {
        self.outcome
    }

    /// In a request, the generation last seen; in an accepted reply, the member's generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// In an accepted reply, the latest close delivered to the home; else 0.
    #[must_use]
    pub fn close(&self) -> u64 {
        self.close
    }

    /// The entity's ID: named in a request, or filled in an accepted reply; `None` when the IRI names it.
    #[must_use]
    pub fn entity(&self) -> Option<SystemId> {
        self.entity
    }

    /// The registration's 48 bytes.
    pub fn encode(&self) -> [u8; REGISTRATION_SIZE] {
        let fields = self
            .partition
            .to_le_bytes()
            .into_iter()
            .chain(self.group.map_or(ABSENT, BucketGroup::get).to_le_bytes())
            .chain(self.iri.unwrap_or(ABSENT).to_le_bytes())
            .chain(self.outcome.to_le_bytes())
            .chain([self.status, 0])
            .chain(self.generation.to_le_bytes())
            .chain(self.close.to_le_bytes())
            .chain(self.entity.map_or([0; 16], SystemId::to_be_bytes));
        let mut out = [0u8; REGISTRATION_SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The registration `bytes` hold; `None` for a group of 2¹⁸ or more but `ABSENT`, or an entity neither all zero
    /// nor a valid system ID. The zero byte and the status's value are unchecked.
    pub(crate) fn decode(bytes: &[u8; REGISTRATION_SIZE]) -> Option<Self> {
        let group = match u32::from_le_bytes(bytes_at(bytes, 4)?) {
            ABSENT => None,
            group => Some(BucketGroup::new(group).ok()?),
        };
        let iri = Some(u32::from_le_bytes(bytes_at(bytes, 8)?)).filter(|iri| *iri != ABSENT);
        let entity: [u8; 16] = bytes_at(bytes, 32)?;
        let entity = if entity == [0; 16] {
            None
        } else {
            Some(SystemId::decode(&entity).ok()?)
        };
        Some(Self {
            partition: u32::from_le_bytes(bytes_at(bytes, 0)?),
            group,
            iri,
            outcome: u16::from_le_bytes(bytes_at(bytes, 12)?),
            status: bytes.get(14).copied()?,
            generation: u64::from_le_bytes(bytes_at(bytes, 16)?),
            close: u64::from_le_bytes(bytes_at(bytes, 24)?),
            entity,
        })
    }

    /// In an accepted reply of a get-only batch, what the home found; `None` in a request, with mode 0 and in a
    /// refused reply.
    #[must_use]
    pub fn status(&self) -> Option<ReferenceStatus> {
        ReferenceStatus::from_number(self.status)
    }

    /// The `status` byte as the registration holds it.
    pub(crate) fn status_byte(&self) -> u8 {
        self.status
    }
}
