/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_mapping::TypeDeclaration;

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_a_type_reads_back_its_parent_and_whether_it_is_abstract() {
    let abstract_root = TypeDeclaration::new(iri("Agent"), None, true);
    assert_eq!(abstract_root.iri(), &iri("Agent"));
    assert_eq!(abstract_root.parent(), None);
    assert!(abstract_root.is_abstract());

    let concrete_child = TypeDeclaration::new(iri("Person"), Some(iri("Agent")), false);
    assert_eq!(concrete_child.iri(), &iri("Person"));
    assert_eq!(concrete_child.parent(), Some(&iri("Agent")));
    assert!(!concrete_child.is_abstract());
}
