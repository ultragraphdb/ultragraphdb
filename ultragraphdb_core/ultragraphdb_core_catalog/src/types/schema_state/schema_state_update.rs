/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema.go and schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use tracing::debug;
use ultragraphdb_core_keys::parse_namespace;
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

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

    /// Dgraph's `Set`: sets the schema of a predicate in memory, logged at debug; [`schema_entry`](crate::schema_entry)
    /// is the entry that stores it.
    pub fn set(&self, pred: &str, schema: pb::SchemaUpdate) {
        debug!(
            attr = pred,
            value_type = value_type(&schema),
            tokenizer = ?schema.tokenizer,
            directive = directive(schema.directive),
            count = schema.count,
            "Setting schema for attr"
        );
        self.write().predicate.insert(pred.to_string(), schema);
    }

    /// Drops a predicate and its background schema from memory alone: what a drop does once it removes the
    /// predicate's keys from the store itself, where Dgraph's `Delete` writes a deletion of the schema key.
    pub fn remove_predicate(&self, pred: &str) {
        let mut s = self.write();
        s.predicate.remove(pred);
        s.mut_schema.remove(pred);
    }

    /// Drops a type from memory alone, as [`SchemaState::remove_predicate`] a predicate.
    pub fn remove_type(&self, type_name: &str) {
        self.write().types.remove(type_name);
    }

    /// Dgraph's `SetMutSchema`: sets the schema being applied to a predicate in the background.
    pub fn set_mut_schema(&self, pred: &str, schema: pb::SchemaUpdate) {
        self.write().mut_schema.insert(pred.to_string(), schema);
    }

    /// Dgraph's `DeleteMutSchema`: drops the background schema of a predicate.
    pub fn delete_mut_schema(&self, pred: &str) {
        self.write().mut_schema.remove(pred);
    }

    /// Dgraph's `SetType`: sets a type definition in memory, logged at debug.
    pub fn set_type(&self, type_name: &str, typ: pb::TypeUpdate) {
        debug!(type_name, typ = ?typ, "Setting type definition for type");
        self.write().types.insert(type_name.to_string(), typ);
    }
}

/// The schema's value type as Dgraph's `logUpdate` writes it: its name, in brackets for a list.
fn value_type(schema: &pb::SchemaUpdate) -> String {
    let name = TypeId::new(schema.value_type).name();
    if schema.list {
        format!("[{name}]")
    } else {
        name.to_string()
    }
}

/// The directive as Go's `%v` writes a protobuf enum: its name, or its number when it has none.
fn directive(value: i32) -> String {
    Directive::try_from(value).map_or_else(|_| value.to_string(), |d| d.as_str_name().to_string())
}
