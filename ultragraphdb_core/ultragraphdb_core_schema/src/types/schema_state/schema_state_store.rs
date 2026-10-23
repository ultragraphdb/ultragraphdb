/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go (Init, Load, LoadFromDb, loadFromDB, Delete, DeleteType). Changed
 * for ultragraphdb: translated to Rust; the store is the caller's `Store`, passed to each call, rather than the
 * package's Badger; the state is loaded from one snapshot, and a deleted definition stays deleted.
 * Copyright (c) 2026. Marvin Hansen.
 */

use prost::Message;
use tracing::{error, info, warn};
use ultragraphdb_core_keys::{parse, schema_key, schema_prefix, type_key, type_prefix};
use ultragraphdb_error_store::go_hex_dump;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use super::SchemaState;
use crate::errors::schema_error::SchemaError;

/// The timestamp the schema is read at, Dgraph's `math.MaxUint64`: the newest version of every key.
const LATEST: u64 = u64::MAX;

/// Dgraph's `Init` resets the package's state and sets the store it reads from. Here [`SchemaState::new`] is the
/// fresh state, and every call that reads or writes the store names it.
impl SchemaState {
    /// Dgraph's `Load`: drops the background schema of `predicate`, then sets the predicate's newest stored schema
    /// in memory. A predicate with no stored schema, or one in a banned namespace, is left as memory holds it.
    ///
    /// # Errors
    ///
    /// `Empty predicate` for an empty name; the [`KeyError`](ultragraphdb_core_keys::KeyError) of a name whose
    /// namespace does not parse, on which Dgraph's `SchemaKey` panics; the store's error; and the decoder's error
    /// for a stored value it refuses, on which Dgraph exits the process.
    pub fn load<S: Store>(&self, store: &S, predicate: &str) -> Result<(), SchemaError> {
        if predicate.is_empty() {
            return Err(SchemaError::State("Empty predicate".into()));
        }
        self.delete_mut_schema(predicate);
        let key = schema_key(predicate)?;
        let Some(item) = store.get_at(&key, LATEST)? else {
            return Ok(());
        };
        let schema = pb::SchemaUpdate::decode(item.value())
            .map_err(|e| SchemaError::Decode(e.to_string()))?;
        self.set(predicate, schema);
        Ok(())
    }

    /// Dgraph's `LoadFromDb`: drops every definition, then sets every stored schema and type, each at its newest
    /// version, read through one snapshot. A key whose newest version is a deletion is absent, and so is a key of a
    /// banned namespace. An empty value is a predicate of the default type, or a type without fields. A key that
    /// does not parse is skipped and logged, as Dgraph skips and logs it.
    ///
    /// Dgraph's stream iterates every version, so the newest version of a deleted key is its deletion, which
    /// `loadFromDB` reads as an empty value: a deleted predicate or type comes back at every restart.
    ///
    /// # Errors
    ///
    /// The store's error, which leaves the definitions loaded before it; Dgraph's stream logs a key whose value it
    /// cannot read and loads without it. `Error while loading schema from db: …` and `Error while loading types
    /// from db: …` for a stored value the decoder refuses, on which Dgraph exits the process.
    pub fn load_from_db<S: Store>(&self, store: &S) -> Result<(), SchemaError> {
        self.delete_all();
        let snapshot = store.snapshot(LATEST)?;
        for item in snapshot.scan(&Scan::prefix(schema_prefix())) {
            let item = item?;
            let Some(attr) = attr_of(item.key()) else {
                continue;
            };
            let schema = if item.value().is_empty() {
                pb::SchemaUpdate {
                    predicate: attr.clone(),
                    value_type: pb::posting::ValType::Default as i32,
                    ..Default::default()
                }
            } else {
                pb::SchemaUpdate::decode(item.value()).map_err(|e| {
                    SchemaError::Decode(format!("Error while loading schema from db: {e}"))
                })?
            };
            self.set(&attr, schema);
        }
        for item in snapshot.scan(&Scan::prefix(type_prefix())) {
            let item = item?;
            let Some(attr) = attr_of(item.key()) else {
                continue;
            };
            let typ = if item.value().is_empty() {
                pb::TypeUpdate {
                    type_name: attr.clone(),
                    ..Default::default()
                }
            } else {
                pb::TypeUpdate::decode(item.value()).map_err(|e| {
                    SchemaError::Decode(format!("Error while loading types from db: {e}"))
                })?
            };
            self.set_type(&attr, typ);
        }
        Ok(())
    }

    /// Dgraph's `Delete`: writes a deletion of the predicate's schema key at `ts`, then drops the predicate and its
    /// background schema from memory. Written as Dgraph writes it, in a batch of its own handed to the operating
    /// system and not synced.
    ///
    /// # Errors
    ///
    /// The [`KeyError`](ultragraphdb_core_keys::KeyError) of a name whose namespace does not parse, on which
    /// Dgraph's `SchemaKey` panics, and the store's error; memory is unchanged by either.
    pub fn delete<S: Store>(&self, store: &S, attr: &str, ts: u64) -> Result<(), SchemaError> {
        let mut s = self.write();
        info!(attr, "Deleting schema for predicate");
        write_deletion(store, schema_key(attr)?, ts)?;
        s.predicate.remove(attr);
        s.mut_schema.remove(attr);
        Ok(())
    }

    /// Dgraph's `DeleteType`: writes a deletion of the type key at `ts`, then drops the type from memory, as
    /// [`SchemaState::delete`] does a predicate.
    ///
    /// # Errors
    ///
    /// As [`SchemaState::delete`]'s, with Dgraph's `TypeKey`.
    pub fn delete_type<S: Store>(
        &self,
        store: &S,
        type_name: &str,
        ts: u64,
    ) -> Result<(), SchemaError> {
        let mut s = self.write();
        info!(type_name, "Deleting type definition for type");
        write_deletion(store, type_key(type_name)?, ts)?;
        s.types.remove(type_name);
        Ok(())
    }
}

/// The namespaced attribute of a schema or type key; `None` for a key that does not parse or names none, logged
/// as Dgraph's `loadFromDB` logs it. A key under either prefix that parses always names its namespace, so the second
/// case is unreachable here as in Dgraph.
fn attr_of(key: &[u8]) -> Option<String> {
    let pk = match parse(key) {
        Ok(pk) => pk,
        Err(e) => {
            error!(key = go_hex_dump(key), error = %e, "Error while parsing key");
            return None;
        }
    };
    if pk.attr().is_empty() {
        warn!(parsed = ?pk, key = hex(key), "Empty Attribute");
        return None;
    }
    Some(pk.attr().to_string())
}

/// Go's `%x` of a byte slice: two lowercase hex digits a byte.
fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_deletion<S: Store>(store: &S, key: Vec<u8>, ts: u64) -> Result<(), SchemaError> {
    let mut batch = WriteBatch::new();
    batch.push(Entry::deletion(key, ts));
    Ok(store.apply(batch, PersistMode::Buffer)?)
}
