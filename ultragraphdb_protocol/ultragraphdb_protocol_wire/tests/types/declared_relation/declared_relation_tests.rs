/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::DeclaredRelation;

#[test]
fn test_a_relation_keeps_its_roles_in_order() {
    let r = DeclaredRelation::new("R", vec!["x".to_string(), "y".to_string()]);
    assert_eq!(r.iri(), "R");
    assert_eq!(r.roles(), &["x".to_string(), "y".to_string()]);
}
