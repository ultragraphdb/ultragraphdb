/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/facet_types.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_dgraph::api::facet::ValType;

/// Dgraph's facet `TypeID`: the type of a facet value, one of the dgo API's facet value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FacetTypeId(ValType);

impl FacetTypeId {
    /// `IntID`.
    pub const INT: FacetTypeId = FacetTypeId(ValType::Int);
    /// `FloatID`.
    pub const FLOAT: FacetTypeId = FacetTypeId(ValType::Float);
    /// `BoolID`.
    pub const BOOL: FacetTypeId = FacetTypeId(ValType::Bool);
    /// `DateTimeID`.
    pub const DATETIME: FacetTypeId = FacetTypeId(ValType::Datetime);
    /// `StringID`.
    pub const STRING: FacetTypeId = FacetTypeId(ValType::String);

    /// Go's `ValTypeForTypeID`. Go panics on a type it does not name; every value of this type names one.
    pub fn val_type(self) -> ValType {
        self.0
    }
}

impl From<ValType> for FacetTypeId {
    fn from(vt: ValType) -> Self {
        FacetTypeId(vt)
    }
}
