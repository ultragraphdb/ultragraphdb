/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_proto_dgraph::api;

/// The object of an N-Quad, built once by [`NQuad::object`](crate::NQuad::object): a uid, blank node or external
/// id, or a value. The wire type keeps both optional, as `api.proto` has them; an edge is built from this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Object<'a> {
    /// The object id: a uid, a blank node or an external id.
    Uid(&'a str),
    /// The object value.
    Value(&'a api::Value),
}
