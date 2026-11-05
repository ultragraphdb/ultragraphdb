/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_traits_store::Entry;

use crate::types::marked_reference::MarkedReference;

/// The key of the mark of `(entity, group)` in `tenant`, in the marks keyspace.
pub(crate) fn mark_key(tenant: u64, entity: SystemId, group: BucketGroup) -> Vec<u8> {
    QuadKey::Mark {
        tenant,
        entity,
        group: group.get(),
    }
    .to_bytes()
}

/// The entry writing `marked`'s mark in `tenant` at its commit version: its generation, 8 bytes big-endian.
pub(crate) fn mark_entry(tenant: u64, marked: &MarkedReference) -> Entry {
    let key = mark_key(tenant, marked.entity(), marked.group());
    Entry::put(key, 0, 0, marked.generation().get().to_be_bytes().to_vec())
}
