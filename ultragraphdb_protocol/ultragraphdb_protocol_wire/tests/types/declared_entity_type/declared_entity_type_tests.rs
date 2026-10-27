/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::DeclaredEntityType;

#[test]
fn test_an_entity_type_keeps_its_parent_and_abstraction() {
    let t = DeclaredEntityType::new("T", Some("U".to_string()), true);
    assert_eq!(
        (t.iri(), t.parent(), t.is_abstract()),
        ("T", Some("U"), true)
    );
    let root = DeclaredEntityType::new("U", None, false);
    assert_eq!((root.parent(), root.is_abstract()), (None, false));
}
