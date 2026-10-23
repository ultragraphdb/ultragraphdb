/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go and schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::parse_namespace;
use ultragraphdb_proto_pb::pb;

use super::SchemaState;
use crate::errors::schema_error::SchemaError;
use crate::parse::parse;

impl SchemaState {
    /// Dgraph's `ParseBytes`: drops every definition, then sets each predicate the schema text parses into. Types
    /// are not set. Dgraph calls it to initialise the state in tests.
    ///
    /// # Errors
    ///
    /// The [`SchemaError`] of [`parse`]; the state is then left empty.
    pub fn parse_text(&self, s: &str) -> Result<(), SchemaError> {
        self.delete_all();
        let result = parse(s)?;
        for update in result.into_parts().0 {
            self.set(&update.predicate.clone(), update);
        }
        Ok(())
    }

    /// Dgraph's `DeleteAll`: drops every predicate, type and background schema.
    pub fn delete_all(&self) {
        let mut s = self.write();
        s.predicate.clear();
        s.types.clear();
        s.mut_schema.clear();
    }

    /// Dgraph's `DeletePredsForNs`: drops the predicates, their background schemas and the types of a namespace. A
    /// name whose namespace does not parse, which only a direct [`set`](Self::set) can store, is in no namespace and
    /// stays; Dgraph exits the process on it.
    pub fn delete_preds_for_ns(&self, del_ns: u64) {
        let mut s = self.write();
        let preds: Vec<String> = s
            .predicate
            .keys()
            .filter(|pred| parse_namespace(pred).is_ok_and(|ns| ns == del_ns))
            .cloned()
            .collect();
        for pred in preds {
            s.predicate.remove(&pred);
            s.mut_schema.remove(&pred);
        }
        s.types
            .retain(|typ, _| !parse_namespace(typ).is_ok_and(|ns| ns == del_ns));
    }

    /// Dgraph's `Set`: sets the schema of a predicate in memory. Schema mutations flow through stage 2's update,
    /// which also stores them.
    pub fn set(&self, pred: &str, schema: pb::SchemaUpdate) {
        self.write().predicate.insert(pred.to_string(), schema);
    }

    /// Dgraph's `SetMutSchema`: sets the schema being applied to a predicate in the background.
    pub fn set_mut_schema(&self, pred: &str, schema: pb::SchemaUpdate) {
        self.write().mut_schema.insert(pred.to_string(), schema);
    }

    /// Dgraph's `DeleteMutSchema`: drops the background schema of a predicate.
    pub fn delete_mut_schema(&self, pred: &str) {
        self.write().mut_schema.remove(pred);
    }

    /// Dgraph's `SetType`: sets a type definition in memory.
    pub fn set_type(&self, type_name: &str, typ: pb::TypeUpdate) {
        self.write().types.insert(type_name.to_string(), typ);
    }
}
