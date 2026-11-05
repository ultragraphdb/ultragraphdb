/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_applier::ChangesetApplier;
use crate::types::entity_ref::EntityRef;
use crate::types::executed::Executed;
use crate::types::registering::Registering;

/// What a partition does as an entity's home (term-dictionary, "A reference to another partition's entity registers
/// at its home"): it gets or creates the entity's ID, and its round writes the referrer-set member.
impl<S, C, E, B> ChangesetApplier<S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// Gets or creates the ID of `entity` in `tenant`: an IRI's as the dictionary resolves it, shared with every
    /// concurrent registrant and creator, its entries held until the registration's round settles them; a known ID as
    /// it is, once the dictionary is found to hold it.
    ///
    /// # Errors
    ///
    /// The dictionary's refusals of creating an ID; the quad store's corruption error for an ID the dictionary does
    /// not hold; the store's error.
    pub fn register(
        &self,
        tenant: u64,
        entity: EntityRef<'_>,
    ) -> Result<Registering, ChangesetError> {
        let quads = self.quads();
        // A registration installs no catalog: it is executed under the empty one, which leaves the installed catalog as
        // it is.
        let mut executed =
            Executed::empty(tenant, quads.lists().applied_ts(), Catalog::new(tenant));
        let id = match entity {
            EntityRef::Iri(iri) => executed.terms.iri(quads, iri)?,
            EntityRef::Id(id) => {
                let _named: (String, bool) = quads.iri_of(tenant, id)?;
                id
            }
        };
        Ok(Registering::new(id, executed))
    }

    /// The execution deriving a home's logged registration of `entity`'s member `group` at `generation` in
    /// `tenant`, for recovery to apply at the record's commit version (write-ahead-log, "Replay derives a home's
    /// referrer-set deltas"), as [`Registering::finish`] derives it.
    pub fn prepare_replayed_registration(
        &self,
        tenant: u64,
        entity: SystemId,
        group: BucketGroup,
        generation: NonZeroU64,
    ) -> Executed {
        let read_ts = self.quads().lists().applied_ts();
        Executed {
            registration: Some((entity, group, generation)),
            ..Executed::empty(tenant, read_ts, Catalog::new(tenant))
        }
    }
}
