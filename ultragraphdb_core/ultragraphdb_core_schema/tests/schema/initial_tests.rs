/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The initial schema and types per namespace, with ACL on and off, against Dgraph's.

use ultragraphdb_core_schema::{
    complete_initial_schema, complete_initial_types, initial_schema, initial_types,
};

use crate::golden_json::{compare_types, compare_updates, initial_cases};

#[test]
fn initial_schema_and_types_are_dgraphs() {
    let golden = initial_cases();
    assert_eq!(golden.cases().len(), 4);
    for c in golden.cases() {
        let ctx = format!("namespace {} acl {}", c.namespace, c.acl_enabled);
        compare_updates(&initial_schema(c.namespace, c.acl_enabled), &c.schema)
            .unwrap_or_else(|m| panic!("{ctx}: {m}"));
        compare_types(&initial_types(c.namespace, c.acl_enabled), &c.types)
            .unwrap_or_else(|m| panic!("{ctx}: {m}"));
        compare_updates(&complete_initial_schema(c.namespace), &c.complete_schema)
            .unwrap_or_else(|m| panic!("{ctx}: {m}"));
        compare_types(&complete_initial_types(c.namespace), &c.complete_types)
            .unwrap_or_else(|m| panic!("{ctx}: {m}"));
    }
}

#[test]
fn complete_initial_schema_is_the_acl_one() {
    for ns in [0, 5] {
        assert_eq!(complete_initial_schema(ns), initial_schema(ns, true));
        assert_eq!(complete_initial_types(ns), initial_types(ns, true));
    }
}

#[test]
fn only_the_root_namespace_has_the_namespace_entries() {
    let names = |ns| -> Vec<String> {
        complete_initial_schema(ns)
            .into_iter()
            .map(|u| u.predicate)
            .collect()
    };
    assert!(names(0).contains(&"0-dgraph.namespace.name".to_string()));
    assert!(names(0).contains(&"0-dgraph.namespace.id".to_string()));
    assert!(
        names(5)
            .iter()
            .all(|p| !p.starts_with("5-dgraph.namespace."))
    );
    assert!(
        complete_initial_types(5)
            .iter()
            .all(|t| t.type_name != "5-dgraph.namespace")
    );
}
