/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, GraphNameRef, Literal, NamedNode, Term};
use ultragraphdb_core_mapping::{
    Change, ChangeRef, Declaration, PredicateDeclaration, StatementRef, StatementView,
};

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

fn iri(local: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{local}"))
}

/// A delete borrows its quad.
#[test]
fn test_a_delete_borrows_its_quad() {
    let (g, s, p) = (iri("g"), iri("s"), iri("p"));
    let o: Term = Literal::from(1).into();
    let delete = Change::Delete {
        graph: g.clone().into(),
        subject: s.clone().into(),
        predicate: p.clone(),
        object: o.clone(),
    };
    assert_eq!(
        delete.as_ref(),
        ChangeRef::Delete {
            graph: g.as_ref().into(),
            subject: s.as_ref().into(),
            predicate: p.as_ref(),
            object: o.as_ref(),
        }
    );
}

/// A statement's delete borrows its graph and reifier.
#[test]
fn test_a_statement_s_delete_borrows_its_reifier() {
    let reifier = iri("r");
    let delete = Change::DeleteStatement {
        graph: GraphName::DefaultGraph,
        reifier: reifier.clone(),
    };
    assert_eq!(
        delete.as_ref(),
        ChangeRef::DeleteStatement {
            graph: GraphNameRef::DefaultGraph,
            reifier: reifier.as_ref(),
        }
    );
}

/// A pattern's delete borrows its subject and predicate.
#[test]
fn test_a_pattern_s_delete_borrows_its_subject_and_predicate() {
    let (s, p) = (iri("s"), iri("p"));
    let delete = Change::DeletePattern {
        graph: GraphName::DefaultGraph,
        subject: s.clone().into(),
        predicate: p.clone(),
    };
    assert_eq!(
        delete.as_ref(),
        ChangeRef::DeletePattern {
            graph: GraphNameRef::DefaultGraph,
            subject: s.as_ref().into(),
            predicate: p.as_ref(),
        }
    );
}

/// A property set on a statement named by its reifier borrows the reifier's view.
#[test]
fn test_a_property_set_borrows_the_reifier_s_view() {
    let (reifier, p) = (iri("r"), iri("since"));
    let value: Term = Literal::from(2020).into();
    let set = Change::SetProperty {
        statement: StatementRef::Reifier {
            graph: GraphName::DefaultGraph,
            reifier: reifier.clone(),
        },
        predicate: p.clone(),
        value: value.clone(),
    };
    assert_eq!(
        set.as_ref(),
        ChangeRef::SetProperty {
            statement: StatementView::Reifier {
                graph: GraphNameRef::DefaultGraph,
                reifier: reifier.as_ref(),
            },
            predicate: p.as_ref(),
            value: value.as_ref(),
        }
    );
}

/// A property removed from a statement named by its triple borrows the triple's view, with its value or with none.
#[test]
fn test_a_property_delete_borrows_the_triple_s_view() {
    let (s, p, o, key) = (iri("s"), iri("p"), iri("o"), iri("since"));
    let value: Term = Literal::from(2020).into();
    let statement = StatementRef::Triple {
        graph: GraphName::DefaultGraph,
        subject: s.clone().into(),
        predicate: p.clone(),
        object: o.clone().into(),
    };
    let view = StatementView::Triple {
        graph: GraphNameRef::DefaultGraph,
        subject: s.as_ref().into(),
        predicate: p.as_ref(),
        object: o.as_ref().into(),
    };
    for removed in [Some(value), None] {
        let delete = Change::DeleteProperty {
            statement: statement.clone(),
            predicate: key.clone(),
            value: removed.clone(),
        };
        assert_eq!(
            delete.as_ref(),
            ChangeRef::DeleteProperty {
                statement: view,
                predicate: key.as_ref(),
                value: removed.as_ref().map(Term::as_ref),
            }
        );
    }
}

/// A declaration is borrowed whole.
#[test]
fn test_a_declaration_is_borrowed_whole() {
    let declaration = Declaration::Predicate(PredicateDeclaration::new(iri("p")));
    let declare = Change::Declare(declaration.clone());
    assert_eq!(declare.as_ref(), ChangeRef::Declare(&declaration));
}
