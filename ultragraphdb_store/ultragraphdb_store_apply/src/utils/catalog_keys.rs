/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::{ENTITY_TYPE, PREDICATE, RELATION};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_mapping::Declaration;

/// The key a write tracks a catalog change of `kind` for `uid` in `tenant` under: a catalog key, written to no
/// store, since a partition holds its catalog in memory alone.
pub(crate) fn catalog_key(tenant: u64, kind: u8, uid: SystemId) -> Vec<u8> {
    QuadKey::Catalog {
        tenant,
        kind,
        id: uid,
    }
    .to_bytes()
}

/// The key a write tracks `declaration` of `uid` in `tenant` under.
pub(crate) fn declaration_key(tenant: u64, uid: SystemId, declaration: &Declaration) -> Vec<u8> {
    let kind = match declaration {
        Declaration::Predicate(_) => PREDICATE,
        Declaration::EntityType(_) => ENTITY_TYPE,
        Declaration::Relation(_) => RELATION,
    };
    catalog_key(tenant, kind, uid)
}
