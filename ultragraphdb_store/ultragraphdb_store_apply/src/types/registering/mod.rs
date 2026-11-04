/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_store_posting::Writes;
use ultragraphdb_traits_store::VersionlessEntry;

use crate::types::executed::Executed;

/// A registration a home accepts, between getting or creating its entity's ID and its round
/// ([`crate::ChangesetApplier::register`]): the entity's ID, and the dictionary entries of an IRI the home created or
/// took while pending, held as an execution's are until its round settles them or it is released.
#[derive(Debug)]
pub struct Registering {
    entity: SystemId,
    executed: Executed,
}

impl Registering {
    pub(crate) fn new(entity: SystemId, executed: Executed) -> Self {
        Self { entity, executed }
    }

    /// The entity's ID.
    pub fn entity(&self) -> SystemId {
        self.entity
    }

    /// What the registration's round applies at its commit version: the referrer-set member of `group` set to
    /// `generation`, its epoch kept when the group is a member and the commit version's epoch otherwise
    /// ([`ultragraphdb_store_quads::ReferrerSet::set`]), derived when the round applies, in its order, so two
    /// registrations of one entity in one round both land; and the dictionary entries of the entity's IRI the home
    /// created. Released, writing nothing, through [`crate::ChangesetApplier::release`].
    pub fn finish(self, group: BucketGroup, generation: NonZeroU64) -> Executed {
        let entity = self.entity;
        let mut executed = self.finish_entity();
        executed.registration = Some((entity, group, generation));
        executed
    }

    /// What a registration naming no bucket group's round applies: the dictionary entries of the entity's IRI the home
    /// created, its whole effect. Released, writing nothing, through [`crate::ChangesetApplier::release`].
    pub fn finish_entity(self) -> Executed {
        let mut executed = self.executed;
        let versionless = executed
            .terms
            .entries()
            .into_iter()
            .map(|(key, value)| VersionlessEntry::put(key, value))
            .collect();
        executed.writes = Writes::replayed(Vec::new(), Vec::new(), versionless, false);
        executed
    }

    /// Whether the registration holds dictionary entries its round must make durable: an ID the home created, or one a
    /// concurrent registrant created and has not yet made durable.
    pub fn holds_entries(&self) -> bool {
        self.executed.terms.written().next().is_some()
    }

    /// The [`Executed`] of a registration answered from the referrer set unchanged, a repeat, which writes nothing: its
    /// release through [`crate::ChangesetApplier::release`] lets go the entries it held.
    pub fn into_executed(self) -> Executed {
        self.executed
    }
}
