/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, GraphNameRef, Literal, NamedNode, NamedNodeRef, Term};
use ultragraphdb_core_mapping::{Change, ChangeRef, Changes, Changeset};

fn insert(name: &str) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: NamedNode::new_unchecked(format!("http://ex.org/{name}")).into(),
        predicate: NamedNode::new_unchecked("http://ex.org/p"),
        object: Literal::from(1).into(),
    }
}

#[test]
fn test_a_changeset_keeps_its_tenant_and_its_changes_in_order() {
    let empty = Changeset::new(7);
    assert!(empty.is_empty());
    assert_eq!(empty.tenant(), 7);

    let mut c = Changeset::new(7).with(insert("a"));
    c.push(insert("b"));
    assert!(!c.is_empty());
    assert_eq!(c.changes(), [insert("a"), insert("b")]);
    assert_eq!(c.into_changes(), vec![insert("a"), insert("b")]);
}

/// As the changes a write applies, a changeset gives its tenant and each change borrowed, in order.
#[test]
fn test_a_changeset_gives_its_changes_borrowed_in_order() {
    let c = Changeset::new(7).with(insert("a")).with(insert("b"));
    let (a, b) = (
        NamedNodeRef::new_unchecked("http://ex.org/a"),
        NamedNodeRef::new_unchecked("http://ex.org/b"),
    );
    let one = Term::from(Literal::from(1));
    let borrowed = |subject: NamedNodeRef<'static>| ChangeRef::Insert {
        graph: GraphNameRef::DefaultGraph,
        subject: subject.into(),
        predicate: NamedNodeRef::new_unchecked("http://ex.org/p"),
        object: one.as_ref(),
    };
    assert_eq!(Changes::tenant(&c), 7);
    assert_eq!(
        Changes::changes(&c).collect::<Vec<_>>(),
        vec![borrowed(a), borrowed(b)]
    );
}
