/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_mapping::RelationDeclaration;

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_a_relation_reads_back_its_iri_and_its_roles_in_order() {
    let r = RelationDeclaration::new(iri("Employment"), vec![iri("employer"), iri("employee")]);
    assert_eq!(r.iri(), &iri("Employment"));
    assert_eq!(r.roles(), [iri("employer"), iri("employee")]);
}
