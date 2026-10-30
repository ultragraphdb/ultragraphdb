/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_traits_store::{Entry, Scan, StoreSnapshot};

use crate::errors::catalog_error::CatalogError;
use crate::types::catalog::Catalog;
use crate::utils::catalog_codec::{ENTITY_TYPE, PREDICATE, RELATION, decode, encode, kind_of};

/// The meta byte of a catalog entry, Dgraph's schema posting bit.
const CATALOG_META: u8 = 0x01;

impl Catalog {
    /// The key of the declaration of `kind` for `uid`.
    pub fn key(namespace: u64, kind: u8, uid: u64) -> Vec<u8> {
        QuadKey::Catalog {
            namespace,
            kind,
            uid,
        }
        .to_bytes()
    }

    /// The entry writing `declaration` of `uid` at `version`, with the discard flag: a request's commit writes it at
    /// its commit timestamp.
    pub fn entry(namespace: u64, uid: u64, declaration: &Declaration, version: u64) -> Entry {
        Entry::put(
            Self::key(namespace, kind_of(declaration), uid),
            version,
            CATALOG_META,
            encode(declaration),
        )
        .with_discard()
    }

    /// The entry writing a declaration's stored bytes `value` under its catalog key `key` at `version`, with the
    /// discard flag, as [`Catalog::entry`] writes it: what a replay of a logged catalog entry writes (7.9).
    pub fn stored_entry(key: Vec<u8>, value: Vec<u8>, version: u64) -> Entry {
        Entry::put(key, version, CATALOG_META, value).with_discard()
    }

    /// The key of predicate `uid`'s declaration.
    pub fn predicate_key(namespace: u64, uid: u64) -> Vec<u8> {
        Self::key(namespace, PREDICATE, uid)
    }

    /// The entry removing the declaration of `kind` for `uid` at `version`.
    pub fn removal(namespace: u64, kind: u8, uid: u64, version: u64) -> Entry {
        Entry::deletion(Self::key(namespace, kind, uid), version)
    }

    /// Every entry of the catalog at `version`, as a drop of everything in the namespace writes its initial catalog.
    pub fn entries(&self, version: u64) -> Vec<Entry> {
        let ns = self.namespace;
        let predicates = self
            .predicates
            .iter()
            .map(|(uid, d)| Self::entry(ns, *uid, &Declaration::Predicate(d.clone()), version));
        let types = self
            .types
            .iter()
            .map(|(uid, d)| Self::entry(ns, *uid, &Declaration::EntityType(d.clone()), version));
        let relations = self
            .relations
            .iter()
            .map(|(uid, d)| Self::entry(ns, *uid, &Declaration::Relation(d.clone()), version));
        predicates.chain(types).chain(relations).collect()
    }

    /// The catalog of `namespace` as `snapshot` reads it, every entry decoded, a type's parent resolved among the
    /// types it holds. A key that does not parse as a catalog key, or an entry that does not decode, is corruption
    /// naming the key and refuses the whole catalog: a declaration skipped would let writes go on under the default
    /// declaration, unchecked and unindexed. Dgraph skips such a key with an error event.
    pub fn load<S: StoreSnapshot>(snapshot: &S, namespace: u64) -> Result<Catalog, CatalogError> {
        let mut catalog = Catalog::new(namespace);
        let mut types = Vec::new();
        let scan = Scan::prefix(key_prefix(Keyspace::Catalog, false, namespace, None, None));
        for item in snapshot.scan(&scan) {
            let item = item?;
            let Ok((QuadKey::Catalog { kind, uid, .. }, None)) = QuadKey::parse(item.key()) else {
                return Err(CatalogError::Corrupted(item.key(), "not a catalog key"));
            };
            match (kind, decode(item.key(), item.value())?) {
                (PREDICATE, Declaration::Predicate(p)) => {
                    catalog.predicates.insert(uid, p);
                }
                (ENTITY_TYPE, Declaration::EntityType(t)) => types.push((uid, t)),
                (RELATION, Declaration::Relation(r)) => {
                    catalog.relations.insert(uid, r);
                }
                _ => {
                    return Err(CatalogError::Corrupted(
                        item.key(),
                        "the entry's kind is not its key's",
                    ));
                }
            }
        }
        catalog.types = types.into_iter().collect();
        Ok(catalog)
    }
}
