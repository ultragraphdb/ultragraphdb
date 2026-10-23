/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_facets::FacetTypeId;
use ultragraphdb_proto_dgraph::api::facet::ValType;

#[test]
fn every_facet_type_id_names_its_val_type() {
    assert_eq!(FacetTypeId::INT.val_type(), ValType::Int);
    assert_eq!(FacetTypeId::FLOAT.val_type(), ValType::Float);
    assert_eq!(FacetTypeId::BOOL.val_type(), ValType::Bool);
    assert_eq!(FacetTypeId::DATETIME.val_type(), ValType::Datetime);
    assert_eq!(FacetTypeId::STRING.val_type(), ValType::String);
}

#[test]
fn a_facet_type_id_converts_from_its_val_type() {
    for vt in [
        ValType::String,
        ValType::Int,
        ValType::Float,
        ValType::Bool,
        ValType::Datetime,
    ] {
        assert_eq!(FacetTypeId::from(vt).val_type(), vt);
    }
}
