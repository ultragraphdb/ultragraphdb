/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_proto_pb::pb;

use crate::types::facet_order::FacetOrder;
use crate::types::filter_tree::FilterTree;

/// What `@facets` parsed into: a facet list with its variables and orders, or a facets filter.
#[derive(Debug, Clone, Default)]
pub(crate) struct FacetRes {
    pub f: Option<pb::FacetParams>,
    pub ft: Option<FilterTree>,
    pub vmap: Option<BTreeMap<String, String>>,
    pub facets_order: Vec<FacetOrder>,
}
