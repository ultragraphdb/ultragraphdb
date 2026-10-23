/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go (mapResponse). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_proto_dgraph::api;

use crate::types::json_value::JsonValue;

/// What parsing a JSON object answers: the node it names, its namespace, and the facets of the edge into it.
#[derive(Debug, Default)]
pub(crate) struct MapResponse {
    /// uid retrieved or allocated for the node.
    pub uid: String,
    /// namespace to which the node belongs.
    pub namespace: u64,
    /// facets on the edge connecting this node to the source if any.
    pub fcts: Vec<api::Facet>,
    /// the object's keys that name a facet, `<predicate>|<facet>`.
    pub raw_facets: BTreeMap<String, JsonValue>,
}
