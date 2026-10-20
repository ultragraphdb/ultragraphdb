/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/mutation.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod nquad_debug;

use std::collections::HashMap;

use ultragraphdb_core_keys::{STAR, ValueTypeInfo, value_type};
use ultragraphdb_core_types::{TypeId, Value, marshal};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;

use crate::errors::dql_error::DqlError;
use crate::mutation::{ERR_INVALID_UID, type_val_from};
use crate::utils::go_strconv::parse_uint;

/// An N-Quad of the API, with its conversion into directed edges: Dgraph's `dql.NQuad`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NQuad(api::NQuad);

impl NQuad {
    pub fn new(nquad: api::NQuad) -> Self {
        Self(nquad)
    }

    /// The API N-Quad.
    pub fn nquad(&self) -> &api::NQuad {
        &self.0
    }

    fn create_edge_prototype(&self, subject_uid: u64) -> pb::DirectedEdge {
        pb::DirectedEdge {
            entity: subject_uid,
            attr: self.0.predicate.clone(),
            namespace: self.0.namespace,
            lang: self.0.lang.clone(),
            facets: self.0.facets.clone(),
            ..pb::DirectedEdge::default()
        }
    }

    /// A directed edge from the subject to the object uid.
    pub fn create_uid_edge(&self, subject_uid: u64, object_uid: u64) -> pb::DirectedEdge {
        let mut out = self.create_edge_prototype(subject_uid);
        out.value_id = object_uid;
        out.value_type = pb::posting::ValType::Uid as i32;
        out
    }

    /// A directed edge from the subject to the N-Quad's value; predicate, language and facets come from the
    /// N-Quad.
    ///
    /// # Errors
    ///
    /// When the value cannot be marshalled to bytes.
    pub fn create_value_edge(&self, subject_uid: u64) -> Result<pb::DirectedEdge, DqlError> {
        let mut out = self.create_edge_prototype(subject_uid);
        self.copy_value(&mut out)?;
        Ok(out)
    }

    /// The edge deleting a whole predicate, from an N-Quad of the form `* p *`.
    ///
    /// # Errors
    ///
    /// When the subject is not `*`, or the value cannot be marshalled.
    pub fn to_delete_pred_edge(&self) -> Result<pb::DirectedEdge, DqlError> {
        // Go also compares the object value's protobuf text with `*`; a value message's text names its field,
        // so it never is, and only the subject decides.
        if self.0.subject != STAR {
            return Err(DqlError::conversion(format!(
                "Subject and object both should be *. Got: {}",
                self.go_debug()
            )));
        }
        let mut out = pb::DirectedEdge {
            // This along with edge.ObjectValue == x.Star would indicate that we want to delete the predicate.
            entity: 0,
            attr: self.0.predicate.clone(),
            namespace: self.0.namespace,
            lang: self.0.lang.clone(),
            facets: self.0.facets.clone(),
            op: pb::directed_edge::Op::Del as i32,
            ..pb::DirectedEdge::default()
        };
        self.copy_value(&mut out)?;
        Ok(out)
    }

    /// The directed edge of the N-Quad, its subject and object resolved to uids: a uid itself, or a blank node or
    /// external id looked up in `new_to_uid`. The operation is left at SET for the caller to set.
    ///
    /// # Errors
    ///
    /// When a uid is zero, malformed or not in the map, the N-Quad has neither value nor object, or the value
    /// cannot be marshalled.
    pub fn to_edge_using(
        &self,
        new_to_uid: &HashMap<String, u64>,
    ) -> Result<pb::DirectedEdge, DqlError> {
        let s_uid = to_uid(&self.0.subject, new_to_uid)?;
        if s_uid == 0 {
            return Err(DqlError::conversion(format!(
                "Subject should be > 0 for nquad: {}",
                self.go_debug()
            )));
        }
        match self.value_type() {
            ValueTypeInfo::Uid => {
                let o_uid = to_uid(&self.0.object_id, new_to_uid)?;
                if o_uid == 0 {
                    return Err(DqlError::conversion(format!(
                        "ObjectId should be > 0 for nquad: {}",
                        self.go_debug()
                    )));
                }
                Ok(self.create_uid_edge(s_uid, o_uid))
            }
            ValueTypeInfo::Plain | ValueTypeInfo::Multi => self.create_value_edge(s_uid),
            _ => Err(DqlError::conversion(format!(
                "Unknown value type for nquad: {}",
                self.go_debug()
            ))),
        }
    }

    fn copy_value(&self, out: &mut pb::DirectedEdge) -> Result<(), DqlError> {
        let (value, t) = self.byte_val()?;
        out.value = value;
        out.value_type = t.enum_value();
        Ok(())
    }

    /// The value as bytes with its type, inferred from the type of the API value. Go dereferences a missing value
    /// and panics; here it is an empty string.
    fn byte_val(&self) -> Result<(Vec<u8>, TypeId), DqlError> {
        let p = match &self.0.object_value {
            Some(v) => type_val_from(v),
            None => type_val_from(&api::Value::default()),
        };
        let tid = p.tid();
        // These three would have already been marshalled to bytes by the client or in the parse function.
        let p = if matches!(tid, TypeId::GEO | TypeId::DATETIME | TypeId::BIGFLOAT) {
            p
        } else {
            marshal(&p, TypeId::BINARY)?
        };
        match p.into_value() {
            Some(Value::Bytes(b)) => Ok((b, tid)),
            _ => Ok((Vec::new(), tid)),
        }
    }

    fn value_type(&self) -> ValueTypeInfo {
        let has_value = self.0.object_value.is_some();
        let has_lang = !self.0.lang.is_empty();
        let has_special_id = self.0.object_id.is_empty();
        value_type(has_value, has_lang, has_special_id)
    }
}

/// The uid of a subject or object: a uid itself, or the uid `new_to_uid` holds for a blank node or external id.
fn to_uid(subject: &str, new_to_uid: &HashMap<String, u64>) -> Result<u64, DqlError> {
    match parse_uint(subject, 0) {
        Ok(0) => Err(DqlError::conversion(ERR_INVALID_UID)),
        Ok(id) => Ok(id),
        // It's an xid.
        Err(_) => new_to_uid.get(subject).copied().ok_or_else(|| {
            DqlError::conversion(format!("UID not found/generated for xid {subject}\n"))
        }),
    }
}
