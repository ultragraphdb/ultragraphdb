/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_store_apply::EntityRef;

/// A reference a transaction makes to an entity another partition owns, which registers at the entity's home before
/// the transaction writes it (term-dictionary, "A reference to another partition's entity registers at its home"):
/// the home, the entity's IRI and its ID when the partition holds one, the subject bucket group referring to it, `None`
/// for a predicate or graph, which joins no referrer set, and the generation the partition last saw for the member, 0
/// for none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    home: PartitionId,
    namespace: u64,
    iri: String,
    entity: Option<SystemId>,
    group: Option<BucketGroup>,
    named: u64,
}

impl Reference {
    /// The reference to `iri` in `namespace`, homed in `home`, known by `entity` if the partition holds its ID, from
    /// `group`, having seen generation `named`.
    pub fn new(
        home: PartitionId,
        namespace: u64,
        iri: String,
        entity: Option<SystemId>,
        group: Option<BucketGroup>,
        named: u64,
    ) -> Self {
        Self {
            home,
            namespace,
            iri,
            entity,
            group,
            named,
        }
    }

    /// The entity's home.
    pub fn home(&self) -> PartitionId {
        self.home
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The entity's IRI.
    pub fn iri(&self) -> &str {
        &self.iri
    }

    /// The entity's ID, if the partition holds it.
    pub fn entity(&self) -> Option<SystemId> {
        self.entity
    }

    /// The entity as the registration names it: by its ID when the partition holds it, by its IRI otherwise.
    pub fn entity_ref(&self) -> EntityRef<'_> {
        match self.entity {
            Some(id) => EntityRef::Id(id),
            None => EntityRef::Iri(&self.iri),
        }
    }

    /// The subject bucket group referring to the entity; `None` for a predicate or graph.
    pub fn group(&self) -> Option<BucketGroup> {
        self.group
    }

    /// The generation the partition last saw for the member, 0 for none.
    pub fn named(&self) -> u64 {
        self.named
    }
}
