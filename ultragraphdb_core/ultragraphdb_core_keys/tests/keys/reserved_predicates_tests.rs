/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reserved, pre-defined, ACL and internal name checks against Dgraph's answers for the same names.

use ultragraphdb_core_keys::{
    all_acl_predicates, attr_in_root_namespace, is_acl_predicate, is_internal_predicate,
    is_other_reserved_predicate, is_pre_defined_predicate, is_pre_defined_type,
    is_registered_reserved_predicate, is_reserved_predicate, is_reserved_type, star_all_predicates,
};

use crate::golden::{ReservedCase, golden};

#[test]
fn test_golden_reserved() {
    let golden = golden::<ReservedCase>("keys_reserved");
    assert!(golden.cases().len() > 30);
    for c in golden.cases() {
        let ns_attr = attr_in_root_namespace(&c.name);
        assert_eq!(
            is_reserved_predicate(&ns_attr).unwrap(),
            c.is_reserved_predicate,
            "{}",
            c.name
        );
        assert_eq!(
            is_pre_defined_predicate(&ns_attr).unwrap(),
            c.is_pre_defined_predicate,
            "{}",
            c.name
        );
        assert_eq!(is_acl_predicate(&c.name), c.is_acl_predicate, "{}", c.name);
        assert_eq!(
            is_other_reserved_predicate(&c.name),
            c.is_other_reserved_predicate,
            "{}",
            c.name
        );
        assert_eq!(
            is_internal_predicate(&ns_attr).unwrap(),
            c.is_internal_predicate,
            "{}",
            c.name
        );
        assert_eq!(
            is_reserved_type(&ns_attr).unwrap(),
            c.is_reserved_type,
            "{}",
            c.name
        );
        assert_eq!(
            is_pre_defined_type(&ns_attr).unwrap(),
            c.is_pre_defined_type,
            "{}",
            c.name
        );
        assert_eq!(
            is_registered_reserved_predicate(&ns_attr).unwrap(),
            c.is_registered_reserved_predicate,
            "{}",
            c.name
        );
    }
}

#[test]
fn test_star_all_predicates() {
    assert_eq!(star_all_predicates(0x85), vec!["85-dgraph.type"]);
}

#[test]
fn test_all_acl_predicates() {
    let mut preds = all_acl_predicates();
    preds.sort();
    assert_eq!(
        preds,
        vec![
            "dgraph.acl.rule",
            "dgraph.password",
            "dgraph.rule.permission",
            "dgraph.rule.predicate",
            "dgraph.user.group",
            "dgraph.xid"
        ]
    );
}

#[test]
fn test_reserved_name_need_not_be_pre_defined() {
    assert!(is_reserved_predicate("0-dgraph.blah").unwrap());
    assert!(!is_pre_defined_predicate("0-dgraph.blah").unwrap());
    assert!(is_reserved_predicate("0-dgraph.type").unwrap());
    assert!(is_pre_defined_predicate("0-dgraph.type").unwrap());
    assert!(!is_reserved_predicate("0-person.name").unwrap());
}

#[test]
fn test_graphql_names_stay_reserved() {
    for pred in [
        "dgraph.graphql.schema",
        "dgraph.graphql.xid",
        "dgraph.graphql.p_query",
    ] {
        assert!(
            is_pre_defined_predicate(&attr_in_root_namespace(pred)).unwrap(),
            "{pred}"
        );
    }
    for typ in ["dgraph.graphql", "dgraph.graphql.persisted_query"] {
        assert!(
            is_pre_defined_type(&attr_in_root_namespace(typ)).unwrap(),
            "{typ}"
        );
    }
}
