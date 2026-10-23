/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go (NQuadBuffer.ParseJSON, mapToNquads, checkForDeletion).
 * Changed for ultragraphdb: translated to Rust; serde_json in place of encoding/json, objects iterated by key.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use serde_json::value::RawValue;
use ultragraphdb_common_utils::{go_strconv, go_utf8};
use ultragraphdb_core_keys::{FACET_DELIMITER, ROOT_NAMESPACE, STAR, predicate_lang};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb::metadata::HintType;

use super::NQuadBuffer;
use crate::errors::chunker_error::{ChunkerError, ChunkerErrorEnum};
use crate::json_parser::{
    get_next_blank, handle_basic_type, handle_geo_type, parse_map_facets, parse_scalar_facets,
    star, strip_spaces,
};
use crate::types::json_op::JsonOp;
use crate::types::json_value::JsonValue;
use crate::types::map_response::MapResponse;
use crate::utils::go_json::replace_lone_surrogates;

/// The first JSON value of the text, `None` at its end; as Go's `json.Decoder.Decode`, what follows is not read.
fn first_value(text: &str) -> Result<Option<(JsonValue, usize)>, ChunkerError> {
    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<&RawValue>();
    match stream.next() {
        None => Ok(None),
        Some(raw) => Ok(Some((JsonValue::decode(raw?)?, stream.byte_offset()))),
    }
}

fn json_error(msg: &str) -> ChunkerError {
    ChunkerError::new(ChunkerErrorEnum::Json(msg.to_string()))
}

fn mutation_error(e: impl ToString) -> ChunkerError {
    ChunkerError::json_mutation(e.to_string())
}

/// The keys of an object that are not facets, the length of Go's map once mapToNquads has moved its facets out.
fn non_facet_len(m: &BTreeMap<String, JsonValue>) -> usize {
    m.keys().filter(|k| !k.contains(FACET_DELIMITER)).count()
}

impl NQuadBuffer {
    /// Dgraph's `NQuadBuffer.ParseJSON`: pushes the N-Quads of a JSON object, or of an array of objects, into the
    /// buffer.
    ///
    /// Each object becomes a node: `uid` names it, a blank node `_:dg.<id>.<n>` when it has none; a nested object
    /// is an edge to its node unless it is a GeoJSON geometry; `<predicate>|<facet>` keys are facets, a uid edge's
    /// in the child object. Objects are read in key order, where Go's maps have none. Bytes that are not UTF-8 and
    /// escaped surrogates that pair with none are read as U+FFFD, as Go reads them; as Go, only the first JSON value
    /// is read, and a root that is not an object is read again after it.
    ///
    /// # Errors
    ///
    /// [`ChunkerErrorEnum::Json`] when the input is not JSON, [`ChunkerErrorEnum::EmptyUid`] for an object to
    /// delete without uid, [`ChunkerErrorEnum::JsonMutation`] for content Dgraph refuses. The N-Quads of objects
    /// parsed before the failure stay in the buffer.
    pub fn parse_json(&mut self, b: &[u8], op: JsonOp) -> Result<(), ChunkerError> {
        let text = go_utf8::go_string(b);
        let text = replace_lone_surrogates(&text);
        let Some((first, end)) = first_value(&text)? else {
            return Err(json_error("EOF"));
        };
        let list = match first {
            JsonValue::Object(ms) => {
                if ms.is_empty() {
                    return Ok(());
                }
                let len = non_facet_len(&ms);
                let mr = self.map_to_nquads(ms, op, "")?;
                self.check_for_deletion(&mr, len, op);
                return Ok(());
            }
            JsonValue::Null => return Ok(()),
            // Go decodes into a map first; when that fails, it writes the input into its reader again and decodes
            // its next value as a list: the rest of the input, then the input once more.
            _ => {
                let again = format!("{}{text}", &text[end..]);
                match first_value(&again)? {
                    None => return Err(json_error("EOF")),
                    Some((JsonValue::Array(list), _)) => list,
                    Some((JsonValue::Null, _)) => return Ok(()),
                    Some((other, _)) => {
                        return Err(json_error(&format!(
                            "json: cannot unmarshal {} into Go value of type []interface {{}}",
                            other.kind()
                        )));
                    }
                }
            }
        };
        for obj in list {
            let JsonValue::Object(m) = obj else {
                return Err(mutation_error("only array of map allowed at root"));
            };
            let len = non_facet_len(&m);
            let mr = self.map_to_nquads(m, op, "")?;
            self.check_for_deletion(&mr, len, op);
        }
        Ok(())
    }

    /// Dgraph's `checkForDeletion`: an object to delete holding only its uid deletes the node's every predicate,
    /// `S * *`.
    fn check_for_deletion(&mut self, mr: &MapResponse, len: usize, op: JsonOp) {
        if op == JsonOp::Delete && !mr.uid.is_empty() && len == 1 && mr.raw_facets.is_empty() {
            self.push(api::NQuad {
                subject: mr.uid.clone(),
                predicate: STAR.to_string(),
                namespace: mr.namespace,
                object_value: Some(star()),
                ..Default::default()
            });
        }
    }

    /// Dgraph's `mapToNquads`: the N-Quads of one JSON object, the node it names and the facets of the edge into
    /// it from `parent_pred`.
    fn map_to_nquads(
        &mut self,
        mut m: BTreeMap<String, JsonValue>,
        op: JsonOp,
        parent_pred: &str,
    ) -> Result<MapResponse, ChunkerError> {
        let mut mr = MapResponse::default();

        // move all facets from global map to smaller mr.raw_facets map.
        let (facets, rest): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut m)
            .into_iter()
            .partition(|(k, _)| k.contains(FACET_DELIMITER));
        mr.raw_facets = facets;
        m = rest;

        // Check field in map.
        if let Some(uid_val) = m.get("uid") {
            let mut uid: u64 = 0;
            match uid_val {
                JsonValue::Number(n) => {
                    uid = go_strconv::parse_int(n, 10).map_err(mutation_error)? as u64
                }
                JsonValue::String(uid_val) => {
                    let s = strip_spaces(uid_val);
                    if uid_val.is_empty() {
                        uid = 0;
                    } else if uid_val.starts_with("_:") {
                        mr.uid = uid_val.clone();
                    } else if s.starts_with("uid(") {
                        mr.uid = s;
                    } else {
                        uid = go_strconv::parse_uint(uid_val, 0).map_err(mutation_error)?;
                    }
                }
                _ => {}
            }
            if uid > 0 {
                mr.uid = uid.to_string();
            }
        }

        if mr.uid.is_empty() {
            if op == JsonOp::Delete {
                // Delete operations with a non-nil value must have an uid specified.
                return Err(ChunkerError::new(ChunkerErrorEnum::EmptyUid));
            }
            mr.uid = get_next_blank();
        }

        let mut namespace = ROOT_NAMESPACE;
        if let Some(ns) = m.get("namespace") {
            match ns {
                JsonValue::Number(n) => {
                    namespace = go_strconv::parse_int(n, 10).map_err(mutation_error)? as u64
                }
                JsonValue::String(ns) => {
                    let s = strip_spaces(ns);
                    namespace = if s.is_empty() {
                        0
                    } else {
                        go_strconv::parse_uint(&s, 0).map_err(mutation_error)?
                    };
                }
                _ => {}
            }
        }
        mr.namespace = namespace;

        for (pred, v) in m {
            // We have already extracted the uid above so we skip that edge. v can be null if the user didn't set a
            // value. We also skip facets here because we parse them with the corresponding predicate.
            if pred == "uid" || pred == "namespace" {
                continue;
            }

            if v == JsonValue::Null {
                if op == JsonOp::Delete {
                    // This corresponds to edge deletion. Here we split predicate and lang directive (ex:
                    // "name@en"), if needed. With JSON mutations that's the only way to send language for a value.
                    let (p, lang) = predicate_lang(&pred);
                    self.push(api::NQuad {
                        subject: mr.uid.clone(),
                        predicate: p.to_string(),
                        namespace,
                        object_value: Some(star()),
                        lang: lang.to_string(),
                        ..Default::default()
                    });
                }
                // If op is set, ignore this triplet and continue.
                continue;
            }

            let mut nq = api::NQuad {
                subject: mr.uid.clone(),
                predicate: pred.clone(),
                namespace,
                ..Default::default()
            };

            let prefix = format!("{pred}{FACET_DELIMITER}");
            if !matches!(v, JsonValue::Array(_)) {
                nq.facets = parse_scalar_facets(&mr.raw_facets, &prefix)?;
            }

            // Here we split predicate and lang directive (ex: "name@en"), if needed. With JSON mutations that's the
            // only way to send language for a value.
            let (p, lang) = predicate_lang(&pred);
            (nq.predicate, nq.lang) = (p.to_string(), lang.to_string());

            match v {
                JsonValue::Number(_) | JsonValue::String(_) | JsonValue::Bool(_) => {
                    handle_basic_type(&pred, &v, op, &mut nq)?;
                    self.push(nq);
                    self.push_pred_hint(&pred, HintType::Single);
                }
                JsonValue::Object(obj) => {
                    if obj.is_empty() {
                        continue;
                    }
                    if handle_geo_type(&obj, &mut nq)? {
                        self.push(nq);
                        self.push_pred_hint(&pred, HintType::Single);
                        continue;
                    }
                    let cr = self.map_to_nquads(obj, op, &pred)?;
                    // Add the connecting edge between the entities.
                    nq.object_id = cr.uid;
                    nq.facets = cr.fcts;
                    self.push(nq);
                    self.push_pred_hint(&pred, HintType::Single);
                }
                JsonValue::Array(items) => {
                    self.push_pred_hint(&pred, HintType::List);
                    self.list_to_nquads(&mr, &pred, namespace, items, op)?;
                }
                JsonValue::Null => {}
            }
        }

        mr.fcts = parse_scalar_facets(&mr.raw_facets, &format!("{parent_pred}{FACET_DELIMITER}"))?;
        Ok(mr)
    }

    /// The N-Quads of a list value. A list item keeps the predicate as written, language tag included, as Dgraph's
    /// does.
    fn list_to_nquads(
        &mut self,
        mr: &MapResponse,
        pred: &str,
        namespace: u64,
        items: Vec<JsonValue>,
        op: JsonOp,
    ) -> Result<(), ChunkerError> {
        let prefix = format!("{pred}{FACET_DELIMITER}");
        // NOTE: facets_map_slice should be empty unless this is a scalar list
        let mut facets_map_slice = Vec::new();
        for (idx, item) in items.into_iter().enumerate() {
            // determine if this is a scalar list
            if idx == 0 && matches!(item, JsonValue::String(_) | JsonValue::Number(_)) {
                facets_map_slice = parse_map_facets(&mr.raw_facets, &prefix)?;
            }

            let mut nq = api::NQuad {
                subject: mr.uid.clone(),
                predicate: pred.to_string(),
                namespace,
                ..Default::default()
            };

            match item {
                JsonValue::String(_) | JsonValue::Number(_) => {
                    handle_basic_type(pred, &item, op, &mut nq)?;
                    // Each map holds one facet, by the index of the value it belongs to.
                    nq.facets = facets_map_slice
                        .iter()
                        .filter_map(|fm| fm.get(&(idx as i64)).cloned())
                        .collect();
                    self.push(nq);
                }
                // A map can mean geojson or a connecting entity.
                JsonValue::Object(obj) => {
                    if handle_geo_type(&obj, &mut nq)? {
                        self.push(nq);
                        continue;
                    }
                    let cr = self.map_to_nquads(obj, op, pred)?;
                    nq.object_id = cr.uid;
                    nq.facets = cr.fcts;
                    self.push(nq);
                }
                _ => {
                    return Err(mutation_error(format!(
                        "got unsupported type for list: {pred}"
                    )));
                }
            }
        }
        Ok(())
    }
}
