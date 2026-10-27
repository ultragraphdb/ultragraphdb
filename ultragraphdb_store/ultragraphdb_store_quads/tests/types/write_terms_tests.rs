/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{BlankNode, Literal, Term};
use ultragraphdb_store_quads::{WriteTerms, iri_key};

use crate::utils::helpers::{ex, quad, quads, write};

/// An IRI new to the store is absent from the dictionary when the write looks it up, and its key is recorded; an IRI
/// the store holds is not.
#[test]
fn test_a_new_iri_is_recorded_absent_and_a_known_one_is_not() {
    let store = quads();
    write(&store, 0, &[quad("known", "p", ex("other"))]);
    let mut terms = WriteTerms::new(0);
    terms
        .iri(store.store(), store.leases(), ex("known").as_str())
        .unwrap();
    terms
        .iri(store.store(), store.leases(), ex("new").as_str())
        .unwrap();
    assert_eq!(
        terms.absent().collect::<Vec<_>>(),
        vec![iri_key(0, ex("new").as_str()).as_slice()]
    );
}

/// A lookup allocating nothing records the IRI it does not find, and nothing for one it does.
#[test]
fn test_a_lookup_that_finds_nothing_is_recorded_absent() {
    let store = quads();
    write(&store, 0, &[quad("known", "p", ex("other"))]);
    let mut terms = WriteTerms::new(0);
    assert!(
        terms
            .find_iri(store.store(), ex("known").as_str())
            .unwrap()
            .is_some()
    );
    assert!(
        terms
            .find_iri(store.store(), ex("missing").as_str())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        terms.absent().collect::<Vec<_>>(),
        vec![iri_key(0, ex("missing").as_str()).as_slice()]
    );
}

/// Each absent key is recorded once and in key order, however often it was looked up, and an IRI the write created
/// is not looked up again.
#[test]
fn test_absent_keys_are_recorded_once_in_key_order() {
    let store = quads();
    let mut terms = WriteTerms::new(3);
    let (s, l) = (store.store(), store.leases());
    terms.iri(s, l, ex("zeta").as_str()).unwrap();
    terms.iri(s, l, ex("alpha").as_str()).unwrap();
    terms.iri(s, l, ex("zeta").as_str()).unwrap();
    terms.find_iri(s, ex("alpha").as_str()).unwrap();
    terms.find_iri(s, ex("beta").as_str()).unwrap();
    terms.find_iri(s, ex("beta").as_str()).unwrap();
    let mut expected = vec![
        iri_key(3, ex("alpha").as_str()),
        iri_key(3, ex("beta").as_str()),
        iri_key(3, ex("zeta").as_str()),
    ];
    expected.sort();
    assert_eq!(
        terms.absent().map(<[u8]>::to_vec).collect::<Vec<_>>(),
        expected
    );
}

/// A literal's datatype IRI is looked up like any IRI, and a blank node, skolemized without a lookup, records nothing;
/// nor does a find of a blank node the write does not name.
#[test]
fn test_a_datatype_is_looked_up_and_a_blank_node_is_not() {
    let store = quads();
    let mut terms = WriteTerms::new(0);
    let (s, l) = (store.store(), store.leases());
    terms.blank(s, l, "b").unwrap();
    assert!(
        terms
            .find(s, Term::from(BlankNode::new_unchecked("other")).as_ref())
            .unwrap()
            .is_none()
    );
    assert_eq!(terms.absent().count(), 0);
    let typed = Literal::new_typed_literal("x", ex("custom"));
    terms.term(s, l, Term::from(typed).as_ref()).unwrap();
    assert_eq!(
        terms.absent().collect::<Vec<_>>(),
        vec![iri_key(0, ex("custom").as_str()).as_slice()]
    );
}
