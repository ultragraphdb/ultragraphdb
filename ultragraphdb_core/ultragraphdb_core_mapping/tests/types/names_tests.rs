/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_mapping::Names;

#[test]
fn test_names_map_to_iris_under_the_base_and_back() {
    let names = Names::new("http://ex.org/");
    assert_eq!(
        names.iri("age"),
        NamedNode::new_unchecked("http://ex.org/age")
    );
    assert_eq!(names.name(&names.iri("age")), "age");
    let other = NamedNode::new_unchecked("http://other.org/age");
    assert_eq!(
        names.name(&other),
        "http://other.org/age",
        "an IRI elsewhere is its own name"
    );
    assert_eq!(names.iri("http://other.org/age"), other);
    assert_eq!(
        names.name(&NamedNode::new_unchecked("http://ex.org/")),
        "http://ex.org/"
    );
    assert_eq!(
        names.node_iri(7),
        NamedNode::new_unchecked("http://ex.org/node/7")
    );
    assert_eq!(names.base(), "http://ex.org/");
    assert_eq!(
        names.skolem_iri(9),
        NamedNode::new_unchecked("http://ex.org/.well-known/genid/9")
    );
}
