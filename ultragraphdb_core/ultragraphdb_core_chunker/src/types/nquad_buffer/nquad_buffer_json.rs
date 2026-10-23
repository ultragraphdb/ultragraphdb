/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go (NQuadBuffer.ParseJSON, mapToNquads, checkForDeletion).
 * Changed for ultragraphdb: translated to Rust; the port of Go's encoding/json in ultragraphdb_common_utils in place
 * of encoding/json, objects iterated by key; the input is one JSON value, decoded once; nested objects are walked
 * on a stack rather than by recursion; list items carry their key's language.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{BTreeMap, HashMap, btree_map};

use ultragraphdb_common_utils::{JsonDecoder, JsonValue, go_strconv};
use ultragraphdb_core_keys::{FACET_DELIMITER, ROOT_NAMESPACE, STAR, predicate_lang};
use ultragraphdb_error_json::JsonError;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb::metadata::HintType;

use super::NQuadBuffer;
use crate::errors::chunker_error::{ChunkerError, ChunkerErrorEnum};
use crate::json_parser::{
    get_next_blank, handle_basic_type, handle_geo_type, parse_map_facets, parse_scalar_facets,
    star, strip_spaces,
};
use crate::types::json_op::JsonOp;
use crate::types::map_response::MapResponse;

/// A JSON object as Go's map holds it.
type Object = BTreeMap<String, JsonValue>;

fn mutation_error(e: impl ToString) -> ChunkerError {
    ChunkerError::json_mutation(e.to_string())
}

/// An object's members as Go's map holds them: the last of duplicate names wins, iterated by name.
fn go_map(v: &mut JsonValue) -> Option<Object> {
    match v {
        JsonValue::Object(members) => Some(std::mem::take(members).into_iter().collect()),
        _ => None,
    }
}

/// The keys of an object that are not facets, the length of Go's map once mapToNquads has moved its facets out.
fn non_facet_len(m: &BTreeMap<String, JsonValue>) -> usize {
    m.keys().filter(|k| !k.contains(FACET_DELIMITER)).count()
}

/// A list value whose items are being turned into N-Quads.
struct ListItems {
    pred: String,
    items: std::vec::IntoIter<JsonValue>,
    idx: usize,
    /// The facets of a scalar list, one map per facet from the item's index to the facet.
    facets: Vec<HashMap<i64, api::Facet>>,
}

/// A JSON object being turned into N-Quads: its node, the members left, the list being read, and the edge into
/// the nested object being read.
struct Node {
    mr: MapResponse,
    parent_pred: String,
    members: btree_map::IntoIter<String, JsonValue>,
    list: Option<ListItems>,
    /// The N-Quad of the edge into the nested object, and the predicate whose single-value hint it records.
    edge: Option<(api::NQuad, Option<String>)>,
}

/// What a node needs next: a nested object read under the predicate, or nothing.
enum Next {
    Child(Object, String),
    Done,
}

impl NQuadBuffer {
    /// Dgraph's `NQuadBuffer.ParseJSON`: pushes the N-Quads of a JSON object, or of an array of objects, into the
    /// buffer.
    ///
    /// Each object becomes a node: `uid` names it, a blank node `_:dg.<id>.<n>` when it has none; a nested object
    /// is an edge to its node unless it is a GeoJSON geometry; `<predicate>|<facet>` keys are facets, a uid edge's
    /// in the child object; a `predicate@lang` key gives every value under it the language, list items included.
    /// Objects are read in key order, where Go's maps have none, and the last of duplicate keys wins. The input is
    /// one JSON value read as Go's `encoding/json` reads it: bytes that are not UTF-8 and escaped surrogates that
    /// pair with none are U+FFFD, and anything but whitespace after the value is refused.
    ///
    /// # Errors
    ///
    /// [`ChunkerErrorEnum::Json`] when the input is not one JSON value or its root is neither an object nor an
    /// array, with Go's text; [`ChunkerErrorEnum::EmptyUid`] for an object to delete without uid,
    /// [`ChunkerErrorEnum::JsonMutation`] for content Dgraph refuses. The N-Quads of objects parsed before the
    /// failure stay in the buffer.
    pub fn parse_json(&mut self, b: &[u8], op: JsonOp) -> Result<(), ChunkerError> {
        let json_error = |e: JsonError| ChunkerError::new(ChunkerErrorEnum::Json(e));
        let mut dec = JsonDecoder::new(b);
        let mut root = dec.decode().map_err(json_error)?;
        dec.check_eof().map_err(json_error)?;
        let list = match &mut root {
            JsonValue::Object(_) => {
                let ms = go_map(&mut root).unwrap_or_default();
                if ms.is_empty() {
                    return Ok(());
                }
                let len = non_facet_len(&ms);
                let mr = self.map_to_nquads(ms, op)?;
                self.check_for_deletion(&mr, len, op);
                return Ok(());
            }
            JsonValue::Null => return Ok(()),
            JsonValue::Array(list) => std::mem::take(list),
            // Go decodes a root that is not a map into a list.
            other => {
                return Err(json_error(JsonError::UnmarshalType(
                    other.kind().to_string(),
                    "[]interface {}".to_string(),
                    String::new(),
                    String::new(),
                )));
            }
        };
        for mut obj in list {
            let Some(m) = go_map(&mut obj) else {
                return Err(mutation_error("only array of map allowed at root"));
            };
            let len = non_facet_len(&m);
            let mr = self.map_to_nquads(m, op)?;
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

    /// Dgraph's `mapToNquads` of a root object: the N-Quads of it and of the objects nested in it, depth first as
    /// Go's recursion reads them, on a stack of the objects being read.
    fn map_to_nquads(
        &mut self,
        m: BTreeMap<String, JsonValue>,
        op: JsonOp,
    ) -> Result<MapResponse, ChunkerError> {
        let mut stack = vec![self.open_node(m, op, String::new())?];
        while let Some(node) = stack.last_mut() {
            match self.next_in_node(node, op)? {
                Next::Child(obj, pred) => {
                    let child = self.open_node(obj, op, pred)?;
                    stack.push(child);
                }
                Next::Done => {
                    let Some(node) = stack.pop() else {
                        break;
                    };
                    let mut mr = node.mr;
                    mr.fcts = parse_scalar_facets(
                        &mr.raw_facets,
                        &format!("{}{FACET_DELIMITER}", node.parent_pred),
                    )?;
                    let Some(parent) = stack.last_mut() else {
                        return Ok(mr);
                    };
                    // Add the connecting edge between the entities.
                    if let Some((mut nq, hint)) = parent.edge.take() {
                        nq.object_id = mr.uid;
                        nq.facets = mr.fcts;
                        self.push(nq);
                        if let Some(pred) = hint {
                            self.push_pred_hint(&pred, HintType::Single);
                        }
                    }
                }
            }
        }
        Ok(MapResponse::default())
    }

    /// The start of Dgraph's `mapToNquads`: the object's facets moved out, the node it names and its namespace.
    fn open_node(
        &mut self,
        m: BTreeMap<String, JsonValue>,
        op: JsonOp,
        parent_pred: String,
    ) -> Result<Node, ChunkerError> {
        let mut mr = MapResponse::default();

        // move all facets from global map to smaller mr.raw_facets map.
        let (facets, m): (BTreeMap<_, _>, BTreeMap<_, _>) = m
            .into_iter()
            .partition(|(k, _)| k.contains(FACET_DELIMITER));
        mr.raw_facets = facets;

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
        Ok(Node {
            mr,
            parent_pred,
            members: m.into_iter(),
            list: None,
            edge: None,
        })
    }

    /// The loop of Dgraph's `mapToNquads` over the object's members: pushes their N-Quads until a member, or a
    /// list item, is an object to read as a node of its own.
    fn next_in_node(&mut self, node: &mut Node, op: JsonOp) -> Result<Next, ChunkerError> {
        loop {
            if let Some(list) = &mut node.list {
                match self.next_in_list(&node.mr, list, op)? {
                    Some((nq, obj)) => {
                        node.edge = Some((nq, None));
                        return Ok(Next::Child(obj, list.pred.clone()));
                    }
                    None => node.list = None,
                }
                continue;
            }
            let Some((pred, mut v)) = node.members.next() else {
                return Ok(Next::Done);
            };
            // We have already extracted the uid above so we skip that edge. v can be null if the user didn't set a
            // value. We also skip facets here because we parse them with the corresponding predicate.
            if pred == "uid" || pred == "namespace" {
                continue;
            }
            let (uid, namespace) = (&node.mr.uid, node.mr.namespace);

            if matches!(v, JsonValue::Null) {
                if op == JsonOp::Delete {
                    // This corresponds to edge deletion. Here we split predicate and lang directive (ex:
                    // "name@en"), if needed. With JSON mutations that's the only way to send language for a value.
                    let (p, lang) = predicate_lang(&pred);
                    self.push(api::NQuad {
                        subject: uid.clone(),
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
                subject: uid.clone(),
                namespace,
                ..Default::default()
            };
            if !matches!(v, JsonValue::Array(_)) {
                let prefix = format!("{pred}{FACET_DELIMITER}");
                nq.facets = parse_scalar_facets(&node.mr.raw_facets, &prefix)?;
            }

            // Here we split predicate and lang directive (ex: "name@en"), if needed. With JSON mutations that's the
            // only way to send language for a value.
            let (p, lang) = predicate_lang(&pred);
            (nq.predicate, nq.lang) = (p.to_string(), lang.to_string());

            match &mut v {
                JsonValue::Number(_) | JsonValue::String(_) | JsonValue::Bool(_) => {
                    handle_basic_type(&pred, &v, op, &mut nq)?;
                    self.push(nq);
                    self.push_pred_hint(&pred, HintType::Single);
                }
                JsonValue::Object(_) => {
                    let obj = go_map(&mut v).unwrap_or_default();
                    if obj.is_empty() {
                        continue;
                    }
                    if handle_geo_type(&obj, &mut nq)? {
                        self.push(nq);
                        self.push_pred_hint(&pred, HintType::Single);
                        continue;
                    }
                    node.edge = Some((nq, Some(pred.clone())));
                    return Ok(Next::Child(obj, pred));
                }
                JsonValue::Array(items) => {
                    self.push_pred_hint(&pred, HintType::List);
                    node.list = Some(ListItems {
                        pred,
                        items: std::mem::take(items).into_iter(),
                        idx: 0,
                        facets: Vec::new(),
                    });
                }
                JsonValue::Null => {}
            }
        }
    }

    /// The items of a list value: pushes their N-Quads until an item is an object to read as a node, returned with
    /// the N-Quad of the edge into it; `None` at the end of the list. Each item carries the language of the key.
    fn next_in_list(
        &mut self,
        mr: &MapResponse,
        list: &mut ListItems,
        op: JsonOp,
    ) -> Result<Option<(api::NQuad, Object)>, ChunkerError> {
        let pred = list.pred.as_str();
        let prefix = format!("{pred}{FACET_DELIMITER}");
        for mut item in list.items.by_ref() {
            let idx = list.idx;
            list.idx += 1;
            // determine if this is a scalar list
            if idx == 0 && matches!(item, JsonValue::String(_) | JsonValue::Number(_)) {
                // NOTE: the facets should be empty unless this is a scalar list
                list.facets = parse_map_facets(&mr.raw_facets, &prefix)?;
            }

            let (p, lang) = predicate_lang(pred);
            let mut nq = api::NQuad {
                subject: mr.uid.clone(),
                predicate: p.to_string(),
                lang: lang.to_string(),
                namespace: mr.namespace,
                ..Default::default()
            };

            match &mut item {
                JsonValue::String(_) | JsonValue::Number(_) => {
                    handle_basic_type(pred, &item, op, &mut nq)?;
                    // Each map holds one facet, by the index of the value it belongs to.
                    nq.facets = list
                        .facets
                        .iter()
                        .filter_map(|fm| fm.get(&(idx as i64)).cloned())
                        .collect();
                    self.push(nq);
                }
                // A map can mean geojson or a connecting entity.
                JsonValue::Object(_) => {
                    let obj = go_map(&mut item).unwrap_or_default();
                    if handle_geo_type(&obj, &mut nq)? {
                        self.push(nq);
                        continue;
                    }
                    return Ok(Some((nq, obj)));
                }
                _ => {
                    return Err(mutation_error(format!(
                        "got unsupported type for list: {pred}"
                    )));
                }
            }
        }
        Ok(None)
    }
}
