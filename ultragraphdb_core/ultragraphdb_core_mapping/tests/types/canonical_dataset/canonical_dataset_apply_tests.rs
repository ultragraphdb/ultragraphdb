/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, MappingError, Statement, StatementId,
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

/// Each statement held, as its subject's, predicate's and object's local names or literal values, with its ID.
fn held(d: &CanonicalDataset) -> Vec<(String, Option<u64>)> {
    d.statements()
        .map(|s| {
            let local = |t: &str| {
                t.strip_prefix("<http://ex.org/")
                    .and_then(|t| t.strip_suffix('>'))
                    .unwrap_or(t)
                    .to_string()
            };
            let graph = match s.graph() {
                GraphName::DefaultGraph => String::new(),
                g => format!(" {}", local(&g.to_string())),
            };
            (
                format!(
                    "{} {} {}{graph}",
                    local(&s.subject().to_string()),
                    local(&s.predicate().to_string()),
                    local(&s.object().to_string())
                ),
                s.id().map(|i| i.value()),
            )
        })
        .collect()
}

fn w(v: i64) -> (NamedNode, Term) {
    (iri("w"), Literal::from(v).into())
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
    assert_eq!(
        s.properties(),
        &[w(1), w(2)],
        "a property value is held once"
    );
    let other = (iri("src"), Term::from(iri("census")));
    apply(
        &mut d,
        vec![
            Change::SetProperty {
                statement: target.clone(),
                predicate: other.0.clone(),
                value: other.1.clone(),
            },
            Change::DeleteProperty {
                statement: target.clone(),
                predicate: iri("w"),
                value: Some(Literal::from(1).into()),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        d.statements().next().unwrap().properties(),
        &[w(2), other.clone()],
        "the value deleted is the one named"
    );
    apply(
        &mut d,
        vec![Change::DeleteProperty {
            statement: target,
            predicate: iri("w"),
            value: None,
        }],
    )
    .unwrap();
    assert_eq!(
        d.statements().next().unwrap().properties(),
        &[other],
        "every value of the predicate goes, and no other predicate's"
    );
    assert_eq!(d.allocated_ids(), 1, "the statement keeps its one ID");
}

#[test]
fn test_a_property_set_on_a_statement_with_an_id_allocates_none_and_finds_it_by_id() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
            Change::SetProperty {
                statement: StatementRef::Id {
                    graph: G,
                    id: StatementId::new(2),
                },
                predicate: iri("w"),
                value: Literal::from(7).into(),
            },
        ],
    )
    .unwrap();
    let properties: Vec<(Option<u64>, Vec<(NamedNode, Term)>)> = d
        .statements()
        .map(|s| (s.id().map(|i| i.value()), s.properties().to_vec()))
        .collect();
    assert_eq!(properties, vec![(Some(1), vec![]), (Some(2), vec![w(7)])]);
    assert_eq!(d.allocated_ids(), 2);
    // An ID names a statement of its own graph only.
    let error = apply(
        &mut d,
        vec![Change::SetProperty {
            statement: StatementRef::Id {
                graph: GraphName::NamedNode(iri("other")),
                id: StatementId::new(2),
            },
            predicate: iri("w"),
            value: Literal::from(8).into(),
        }],
    )
    .unwrap_err();
    assert_eq!(
        error,
        MappingError::NoSuchStatement("2 in <http://ex.org/other>".into())
    );
}

#[test]
fn test_an_edge_repeating_an_inserted_triple_gives_both_statements_ids() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            insert("a", "knows", iri("b")),
            edge("a", "knows", "b", vec![]),
        ],
    )
    .unwrap();
    assert_eq!(ids(&d), vec![Some(1), Some(2)]);
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
    assert_eq!(
        held(&d),
        vec![
            ("a knows b".into(), Some(2)),
            ("a knows c".into(), None),
            ("a nick \"x\"".into(), None),
        ],
        "the statement named by its ID goes, and its sibling stays"
    );
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
    assert_eq!(held(&d), vec![("a nick \"x\"".into(), None)]);
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
fn test_deleting_a_triples_last_statement_removes_the_triple() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
        ],
    )
    .unwrap();
    let delete = |id| Change::DeleteStatement {
        graph: G,
        id: StatementId::new(id),
    };
    apply(&mut d, vec![delete(2), delete(1)]).unwrap();
    assert_eq!((d.triple_count(), d.statements().count()), (0, 0));
    // A deleted ID is not reissued.
    apply(&mut d, vec![edge("a", "knows", "b", vec![w(1)])]).unwrap();
    assert_eq!(ids(&d), vec![Some(3)]);
}

#[test]
fn test_a_triple_delete_removes_every_statement_holding_it() {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![w(1)]),
            edge("a", "knows", "c", vec![]),
            Change::Delete {
                graph: G,
                subject: node("a"),
                predicate: iri("knows"),
                object: iri("b").into(),
            },
            // A triple not held is no error and changes nothing.
            Change::Delete {
                graph: G,
                subject: node("z"),
                predicate: iri("knows"),
                object: iri("b").into(),
            },
        ],
    )
    .unwrap();
    assert_eq!(held(&d), vec![("a knows c".into(), None)]);
}

#[test]
fn test_a_pattern_delete_takes_only_its_graph_subject_and_predicate() {
    let mut d = CanonicalDataset::new();
    let other = GraphName::NamedNode(iri("g2"));
    apply(
        &mut d,
        vec![
            insert("a", "knows", iri("b")),
            insert("a", "knows", iri("c")),
            insert("b", "knows", iri("c")),
            insert("a", "likes", iri("c")),
            Change::Insert {
                graph: other.clone(),
                subject: node("a"),
                predicate: iri("knows"),
                object: iri("b").into(),
            },
            Change::DeletePattern {
                graph: G,
                subject: node("a"),
                predicate: iri("knows"),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        held(&d),
        vec![
            ("a knows b g2".into(), None),
            ("a likes c".into(), None),
            ("b knows c".into(), None),
        ]
    );
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
fn test_a_single_valued_write_replaces_only_its_subjects_object_in_its_graph() {
    let mut d = CanonicalDataset::new();
    let other = GraphName::NamedNode(iri("g2"));
    apply(
        &mut d,
        vec![
            insert("a", "age", Literal::from(30)),
            insert("b", "age", Literal::from(40)),
            Change::Insert {
                graph: other.clone(),
                subject: node("a"),
                predicate: iri("age"),
                object: Literal::from(50).into(),
            },
            insert("a", "nick", Literal::new_simple_literal("x")),
            insert("a", "nick", Literal::new_simple_literal("y")),
            insert("a", "age", Literal::from(31)),
        ],
    )
    .unwrap();
    // `"30"` is replaced; b's age, a's age in g2 and a's multi-valued nicks are not.
    let typed = |v: i64| format!("\"{v}\"^^<http://www.w3.org/2001/XMLSchema#integer>");
    assert_eq!(
        held(&d),
        vec![
            (format!("a age {}", typed(31)), None),
            (format!("a age {} g2", typed(50)), None),
            ("a nick \"x\"".into(), None),
            ("a nick \"y\"".into(), None),
            (format!("b age {}", typed(40)), None),
        ]
    );
    // An edge of a single-valued predicate replaces as an insert does.
    apply(
        &mut d,
        vec![Change::InsertEdge {
            graph: G,
            subject: node("a"),
            predicate: iri("age"),
            object: Literal::from(32).into(),
            properties: vec![w(1)],
        }],
    )
    .unwrap();
    assert_eq!(held(&d)[0], (format!("a age {}", typed(32)), Some(1)));
    assert_eq!(d.triple_count(), 5);
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
    assert_eq!(
        error,
        MappingError::AmbiguousStatement(
            "<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b>".into()
        )
    );
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
    assert_eq!(error, MappingError::NoSuchStatement("99 in DEFAULT".into()));
    assert_eq!(d, before);
    let unheld = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("knows"),
        object: iri("z").into(),
    };
    let error = apply(
        &mut d,
        vec![
            insert("x", "p", iri("y")),
            Change::SetProperty {
                statement: unheld,
                predicate: iri("w"),
                value: Literal::from(1).into(),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        error,
        MappingError::NoSuchStatement(
            "<http://ex.org/a> <http://ex.org/knows> <http://ex.org/z>".into()
        )
    );
    assert_eq!(d, before);
    // A triple term, as an insert's object and as an edge's, named as oxrdf writes the triple.
    let triple = Triple::new(node("a"), iri("p"), iri("b"));
    let refused = MappingError::UnsupportedObject(triple.to_string());
    let error = apply(
        &mut d,
        vec![insert("a", "says", Term::Triple(Box::new(triple.clone())))],
    )
    .unwrap_err();
    assert_eq!(error, refused);
    let error = apply(
        &mut d,
        vec![
            edge("x", "p", "y", vec![w(1)]),
            Change::InsertEdge {
                graph: G,
                subject: node("a"),
                predicate: iri("says"),
                object: Term::Triple(Box::new(triple)),
                properties: vec![],
            },
        ],
    )
    .unwrap_err();
    assert_eq!(error, refused);
    assert_eq!(d, before, "no statement and no ID of a refused changeset");
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
