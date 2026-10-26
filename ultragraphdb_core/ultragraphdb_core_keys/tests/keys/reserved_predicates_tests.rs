/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reserved, pre-defined, ACL and internal name checks.

use ultragraphdb_core_keys::{
    KeyError, all_acl_predicates, attr_in_root_namespace, is_internal_predicate,
    is_pre_defined_predicate, is_pre_defined_type, is_registered_reserved_predicate,
    is_reserved_predicate, is_reserved_type, star_all_predicates,
};

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

#[test]
fn test_namespaced_checks_refuse_a_bare_name() {
    // Dgraph panics indexing past the split of a name without the namespace separator.
    type Check = fn(&str) -> Result<bool, KeyError>;
    let checks: [(&str, Check); 6] = [
        ("is_reserved_predicate", is_reserved_predicate),
        ("is_pre_defined_predicate", is_pre_defined_predicate),
        ("is_internal_predicate", is_internal_predicate),
        ("is_reserved_type", is_reserved_type),
        ("is_pre_defined_type", is_pre_defined_type),
        (
            "is_registered_reserved_predicate",
            is_registered_reserved_predicate,
        ),
    ];
    for (name, check) in checks {
        for bare in ["dgraph.type", "uid", ""] {
            assert_eq!(
                check(bare),
                Err(KeyError::MissingNamespaceSeparator(bare.to_string())),
                "{name}({bare:?})"
            );
        }
    }
}

#[test]
fn test_checks_read_only_the_predicate() {
    // The namespace does not change a check's answer, and a separator inside the predicate is part of it.
    for ns in [0u64, 1, 0x85, u64::MAX] {
        let attr = |name: &str| ultragraphdb_core_keys::namespace_attr(ns, name);
        assert!(is_reserved_predicate(&attr("dgraph.type")).unwrap(), "{ns}");
        assert!(
            is_pre_defined_predicate(&attr("dgraph.type")).unwrap(),
            "{ns}"
        );
        assert!(is_internal_predicate(&attr("uid")).unwrap(), "{ns}");
        assert!(
            is_pre_defined_type(&attr("dgraph.graphql")).unwrap(),
            "{ns}"
        );
        assert!(
            !is_reserved_predicate(&attr("a-dgraph.type")).unwrap(),
            "{ns}"
        );
    }
    assert_eq!(
        star_all_predicates(u64::MAX),
        vec!["ffffffffffffffff-dgraph.type"]
    );
    assert_eq!(star_all_predicates(0), vec!["0-dgraph.type"]);
}
