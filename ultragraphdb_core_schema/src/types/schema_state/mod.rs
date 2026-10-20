/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod schema_state_query;
mod schema_state_update;

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use ultragraphdb_proto_pb::pb;

use crate::types::schema_mode::SchemaMode;

/// Dgraph's schema `state`: the current predicate and type definitions of every namespace, keyed by namespaced
/// attribute (`0-name`), and the schemas being applied in the background while indexes are built.
///
/// Every method takes `&self` and locks internally, as Dgraph's `sync.RWMutex` does, so one state can be shared
/// between threads. Loading it from and storing it to disk is stage 2's.
#[derive(Debug, Default)]
pub struct SchemaState {
    inner: RwLock<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Predicate to its schema.
    predicate: HashMap<String, pb::SchemaUpdate>,
    /// Type name to its definition.
    types: HashMap<String, pb::TypeUpdate>,
    /// The schema update being applied in the background, per predicate.
    mut_schema: HashMap<String, pb::SchemaUpdate>,
}

impl Inner {
    /// The schema of `pred` for `mode`: in write mode the background one where there is one, else the current one.
    fn schema_for(&self, mode: SchemaMode, pred: &str) -> Option<&pb::SchemaUpdate> {
        let pending = match mode {
            SchemaMode::Write => self.mut_schema.get(pred),
            SchemaMode::Query => None,
        };
        pending.or_else(|| self.predicate.get(pred))
    }

    /// Whether `check` holds for the background schema of `pred` in write mode, or else for its current schema.
    fn holds(
        &self,
        mode: SchemaMode,
        pred: &str,
        check: impl Fn(&pb::SchemaUpdate) -> bool,
    ) -> bool {
        if mode == SchemaMode::Write && self.mut_schema.get(pred).is_some_and(&check) {
            return true;
        }
        self.predicate.get(pred).is_some_and(check)
    }
}

impl SchemaState {
    /// An empty state.
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
    }
}
