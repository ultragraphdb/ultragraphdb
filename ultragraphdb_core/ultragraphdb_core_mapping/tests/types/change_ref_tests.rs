/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, GraphNameRef, Literal, NamedNode, Term};
use ultragraphdb_core_mapping::{Change, ChangeRef};

/// An edge with properties borrows each property, in order, with its value.
#[test]
fn test_an_edge_borrows_its_properties_in_order() {
    let (s, p, o) = (
        NamedNode::new_unchecked("http://ex.org/s"),
        NamedNode::new_unchecked("http://ex.org/p"),
        NamedNode::new_unchecked("http://ex.org/o"),
    );
    let (since, until) = (
        NamedNode::new_unchecked("http://ex.org/since"),
        NamedNode::new_unchecked("http://ex.org/until"),
    );
    let (from, to): (Term, Term) = (Literal::from(2020).into(), Literal::from(2030).into());
    let edge = Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: s.clone().into(),
        predicate: p.clone(),
        object: o.clone().into(),
        properties: vec![(since.clone(), from.clone()), (until.clone(), to.clone())],
    };
    assert_eq!(
        edge.as_ref(),
        ChangeRef::InsertEdge {
            graph: GraphNameRef::DefaultGraph,
            subject: s.as_ref().into(),
            predicate: p.as_ref(),
            object: o.as_ref().into(),
            properties: vec![
                (since.as_ref(), from.as_ref()),
                (until.as_ref(), to.as_ref())
            ],
        }
    );
}
