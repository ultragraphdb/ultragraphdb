/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, MappingErrorEnum, Statement, StatementId,
    StatementRef,
};

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

fn node(name: &str) -> NamedOrBlankNode {
    iri(name).into()
}

const G: GraphName = GraphName::DefaultGraph;

fn insert(s: &str, p: &str, o: impl Into<Term>) -> Change {
    Change::Insert {
        graph: G,
        subject: node(s),
        predicate: iri(p),
        object: o.into(),
    }
}

fn edge(s: &str, p: &str, o: &str, properties: Vec<(NamedNode, Term)>) -> Change {
    Change::InsertEdge {
        graph: G,
        subject: node(s),
        predicate: iri(p),
        object: iri(o).into(),
        properties,
    }
}

fn apply(
    d: &mut CanonicalDataset,
    changes: Vec<Change>,
) -> Result<(), ultragraphdb_core_mapping::MappingError> {
    let changeset = changes.into_iter().fold(Changeset::new(0), Changeset::with);
    d.apply(&changeset, |p: &NamedNode| {
        if *p == iri("age") {
            Cardinality::Single
        } else {
            Cardinality::Multi
        }
    })
}

fn ids(d: &CanonicalDataset) -> Vec<Option<u64>> {
    d.statements().map(|s| s.id().map(|i| i.value())).collect()
}

#[test]
fn test_a_lone_edge_takes_no_id_and_a_repeat_gives_both_one() {
    let mut d = CanonicalDataset::new();
    apply(&mut d, vec![edge("a", "knows", "b", vec![])]).unwrap();
    assert_eq!(ids(&d), vec![None]);
    apply(&mut d, vec![edge("a", "knows", "b", vec![])]).unwrap();
    assert_eq!(ids(&d), vec![Some(1), Some(2)]);
    apply(&mut d, vec![edge("a", "knows", "b", vec![])]).unwrap();
    assert_eq!(ids(&d), vec![Some(1), Some(2), Some(3)]);
    assert_eq!(d.triple_count(), 1);
    assert_eq!(d.allocated_ids(), 3);
}

#[test]
fn test_an_edge_with_properties_takes_an_id_and_inserts_never_repeat() {
    let mut d = CanonicalDataset::new();
    let since = (iri("since"), Term::from(Literal::from(2020)));
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![since.clone()]),
            insert("a", "knows", iri("b")),
        ],
    )
    .unwrap();
    let held: Vec<Statement> = d.statements().collect();
    assert_eq!(held.len(), 1, "an insert of a held triple adds nothing");
    assert_eq!(held[0].id(), Some(StatementId::new(1)));
    assert_eq!(held[0].properties(), &[since]);
}

#[test]
fn test_a_property_set_on_a_plain_statement_gives_it_an_id_and_its_delete_removes_one_value() {
    let mut d = CanonicalDataset::new();
    let target = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("knows"),
        object: iri("b").into(),
    };
    let set = |v: i64| Change::SetProperty {
        statement: target.clone(),
        predicate: iri("w"),
        value: Literal::from(v).into(),
    };
    apply(
        &mut d,
        vec![insert("a", "knows", iri("b")), set(1), set(2), set(2)],
    )
    .unwrap();
    let s = d.statements().next().unwrap();
    assert_eq!(s.id(), Some(StatementId::new(1)));
    assert_eq!(s.properties().len(), 2, "a property value is held once");
    apply(
        &mut d,
        vec![Change::DeleteProperty {
            statement: target.clone(),
            predicate: iri("w"),
            value: Some(Literal::from(1).into()),
        }],
    )
    .unwrap();
    assert_eq!(d.statements().next().unwrap().properties().len(), 1);
    apply(
        &mut d,
        vec![Change::DeleteProperty {
            statement: target,
            predicate: iri("w"),
            value: None,
        }],
    )
    .unwrap();
    assert!(d.statements().next().unwrap().properties().is_empty());
}

#[test]
fn test_deletes_remove_a_triple_a_statement_or_a_pattern() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
            insert("a", "knows", iri("c")),
            insert("a", "nick", Literal::new_simple_literal("x")),
        ],
    )
    .unwrap();
    apply(
        &mut d,
        vec![Change::DeleteStatement {
            graph: G,
            id: StatementId::new(1),
        }],
    )
    .unwrap();
    assert_eq!(d.statements().count(), 3);
    apply(
        &mut d,
        vec![Change::DeleteStatement {
            graph: GraphName::NamedNode(iri("other")),
            id: StatementId::new(2),
        }],
    )
    .unwrap();
    assert_eq!(
        d.statements().count(),
        3,
        "an ID names a statement of its graph only"
    );
    apply(
        &mut d,
        vec![Change::DeletePattern {
            graph: G,
            subject: node("a"),
            predicate: iri("knows"),
        }],
    )
    .unwrap();
    assert_eq!(d.statements().count(), 1);
    apply(
        &mut d,
        vec![Change::Delete {
            graph: G,
            subject: node("a"),
            predicate: iri("nick"),
            object: Literal::new_simple_literal("x").into(),
        }],
    )
    .unwrap();
    assert_eq!(d.statements().count(), 0);
}

#[test]
fn test_a_single_valued_insert_replaces_other_objects_and_keeps_a_rewritten_one() {
    let mut d = CanonicalDataset::new();
    let target = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("age"),
        object: Literal::from(30).into(),
    };
    apply(
        &mut d,
        vec![
            insert("a", "age", Literal::from(30)),
            Change::SetProperty {
                statement: target,
                predicate: iri("src"),
                value: iri("census").into(),
            },
            insert("a", "age", Literal::from(30)),
        ],
    )
    .unwrap();
    assert_eq!(
        d.statements().next().unwrap().properties().len(),
        1,
        "writing the value again keeps its statement"
    );
    apply(&mut d, vec![insert("a", "age", Literal::from(31))]).unwrap();
    let held: Vec<Statement> = d.statements().collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].object(), &Term::from(Literal::from(31)));
}

#[test]
fn test_a_refused_change_leaves_nothing_of_its_changeset() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
        ],
    )
    .unwrap();
    let before = d.clone();
    let ambiguous = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("knows"),
        object: iri("b").into(),
    };
    let error = apply(
        &mut d,
        vec![
            insert("x", "p", iri("y")),
            Change::SetProperty {
                statement: ambiguous,
                predicate: iri("w"),
                value: Literal::from(1).into(),
            },
        ],
    )
    .unwrap_err();
    assert!(matches!(
        error.0,
        MappingErrorEnum::AmbiguousStatement { .. }
    ));
    assert_eq!(d, before);
    let missing = StatementRef::Id {
        graph: G,
        id: StatementId::new(99),
    };
    let error = apply(
        &mut d,
        vec![Change::DeleteProperty {
            statement: missing,
            predicate: iri("w"),
            value: None,
        }],
    )
    .unwrap_err();
    assert!(matches!(error.0, MappingErrorEnum::NoSuchStatement { .. }));
    let triple = Term::Triple(Box::new(Triple::new(node("a"), iri("p"), iri("b"))));
    let error = apply(&mut d, vec![insert("a", "says", triple)]).unwrap_err();
    assert!(matches!(
        error.0,
        MappingErrorEnum::UnsupportedObject { .. }
    ));
    assert_eq!(d, before);
}

#[test]
fn test_declarations_change_no_statement() {
    let mut d = CanonicalDataset::new();
    let declaration = ultragraphdb_core_mapping::Declaration::Predicate(
        ultragraphdb_core_mapping::PredicateDeclaration::new(iri("p")),
    );
    apply(&mut d, vec![Change::Declare(declaration)]).unwrap();
    assert_eq!(d, CanonicalDataset::new());
}
