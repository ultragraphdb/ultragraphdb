/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, MappingError, Statement, StatementRef,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The dataset's reifier of its `n`th statement ID, `ugdb:statement/{n}`, as its documentation names it.
fn model_reifier(n: u64) -> NamedNode {
    NamedNode::new_unchecked(format!("https://w3id.org/ultragraphdb/ns/statement/{n}"))
}

/// The refusal of a property change naming the model's `n`th reifier in `graph`, which holds no such statement.
fn no_statement(n: u64, graph: &str) -> MappingError {
    MappingError::no_such_statement(format!("{} in {graph}", model_reifier(n)))
}

/// The number a model reifier names.
fn model_number(reifier: &NamedNode) -> u64 {
    reifier
        .as_str()
        .rsplit('/')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

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

/// The error `apply` refuses `changes` with.
fn refusal(d: &mut CanonicalDataset, changes: Vec<Change>) -> TestResult<MappingError> {
    match apply(d, changes) {
        Ok(()) => Err(TestError::new(
            "the changeset was applied where it should be refused",
        )),
        Err(error) => Ok(error),
    }
}

fn ids(d: &CanonicalDataset) -> Vec<Option<u64>> {
    d.statements()
        .map(|s| s.reifier().map(model_number))
        .collect()
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
                g @ (GraphName::NamedNode(_) | GraphName::BlankNode(_)) => {
                    format!(" {}", local(&g.to_string()))
                }
            };
            (
                format!(
                    "{} {} {}{graph}",
                    local(&s.subject().to_string()),
                    local(&s.predicate().to_string()),
                    local(&s.object().to_string())
                ),
                s.reifier().map(model_number),
            )
        })
        .collect()
}

fn w(v: i64) -> (NamedNode, Term) {
    (iri("w"), Literal::from(v).into())
}

#[test]
fn test_a_lone_edge_takes_no_id_and_a_repeat_gives_both_one() -> TestResult {
    let mut d = CanonicalDataset::new();
    apply(&mut d, vec![edge("a", "knows", "b", vec![])])?;
    assert_eq!(ids(&d), vec![None]);
    apply(&mut d, vec![edge("a", "knows", "b", vec![])])?;
    assert_eq!(ids(&d), vec![Some(1), Some(2)]);
    apply(&mut d, vec![edge("a", "knows", "b", vec![])])?;
    assert_eq!(ids(&d), vec![Some(1), Some(2), Some(3)]);
    assert_eq!(d.triple_count(), 1);
    assert_eq!(d.allocated_ids(), 3);
    Ok(())
}

#[test]
fn test_an_edge_with_properties_takes_an_id_and_inserts_never_repeat() -> TestResult {
    let mut d = CanonicalDataset::new();
    let since = (iri("since"), Term::from(Literal::from(2020)));
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![since.clone()]),
            insert("a", "knows", iri("b")),
        ],
    )?;
    let held: Vec<Statement> = d.statements().collect();
    assert_eq!(held.len(), 1, "an insert of a held triple adds nothing");
    assert_eq!(held[0].reifier(), Some(&model_reifier(1)));
    assert_eq!(held[0].properties(), &[since]);
    Ok(())
}

#[test]
fn test_a_property_set_on_a_plain_statement_gives_it_an_id_and_its_delete_removes_one_value()
-> TestResult {
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
    )?;
    let s = d.statements().next().context("a statement")?;
    assert_eq!(s.reifier(), Some(&model_reifier(1)));
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
    )?;
    assert_eq!(
        d.statements().next().context("a statement")?.properties(),
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
    )?;
    assert_eq!(
        d.statements().next().context("a statement")?.properties(),
        &[other],
        "every value of the predicate goes, and no other predicate's"
    );
    assert_eq!(d.allocated_ids(), 1, "the statement keeps its one ID");
    Ok(())
}

/// Each statement's ID, if it has one, and its properties.
type StatementProperties = Vec<(Option<u64>, Vec<(NamedNode, Term)>)>;

#[test]
fn test_a_property_set_on_a_statement_with_an_id_allocates_none_and_finds_it_by_id() -> TestResult {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
            Change::SetProperty {
                statement: StatementRef::Reifier {
                    graph: G,
                    reifier: model_reifier(2),
                },
                predicate: iri("w"),
                value: Literal::from(7).into(),
            },
        ],
    )?;
    let properties: StatementProperties = d
        .statements()
        .map(|s| (s.reifier().map(model_number), s.properties().to_vec()))
        .collect();
    assert_eq!(properties, vec![(Some(1), vec![]), (Some(2), vec![w(7)])]);
    assert_eq!(d.allocated_ids(), 2);
    // An ID names a statement of its own graph only.
    let error = refusal(
        &mut d,
        vec![Change::SetProperty {
            statement: StatementRef::Reifier {
                graph: GraphName::NamedNode(iri("other")),
                reifier: model_reifier(2),
            },
            predicate: iri("w"),
            value: Literal::from(8).into(),
        }],
    )?;
    assert_eq!(error, no_statement(2, "<http://ex.org/other>"));
    Ok(())
}

#[test]
fn test_an_edge_repeating_an_inserted_triple_gives_both_statements_ids() -> TestResult {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            insert("a", "knows", iri("b")),
            edge("a", "knows", "b", vec![]),
        ],
    )?;
    assert_eq!(ids(&d), vec![Some(1), Some(2)]);
    Ok(())
}

#[test]
fn test_deletes_remove_a_triple_a_statement_or_a_pattern() -> TestResult {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
            insert("a", "knows", iri("c")),
            insert("a", "nick", Literal::new_simple_literal("x")),
        ],
    )?;
    apply(
        &mut d,
        vec![Change::DeleteStatement {
            graph: G,
            reifier: model_reifier(1),
        }],
    )?;
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
            reifier: model_reifier(2),
        }],
    )?;
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
    )?;
    assert_eq!(held(&d), vec![("a nick \"x\"".into(), None)]);
    apply(
        &mut d,
        vec![Change::Delete {
            graph: G,
            subject: node("a"),
            predicate: iri("nick"),
            object: Literal::new_simple_literal("x").into(),
        }],
    )?;
    assert_eq!(d.statements().count(), 0);
    Ok(())
}

#[test]
fn test_deleting_a_triples_last_statement_removes_the_triple() -> TestResult {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
        ],
    )?;
    let delete = |id| Change::DeleteStatement {
        graph: G,
        reifier: model_reifier(id),
    };
    apply(&mut d, vec![delete(2), delete(1)])?;
    assert_eq!((d.triple_count(), d.statements().count()), (0, 0));
    // A deleted ID is not reissued.
    apply(&mut d, vec![edge("a", "knows", "b", vec![w(1)])])?;
    assert_eq!(ids(&d), vec![Some(3)]);
    Ok(())
}

#[test]
fn test_a_triple_delete_removes_every_statement_holding_it() -> TestResult {
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
    )?;
    assert_eq!(held(&d), vec![("a knows c".into(), None)]);
    Ok(())
}

#[test]
fn test_a_pattern_delete_takes_only_its_graph_subject_and_predicate() -> TestResult {
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
    )?;
    assert_eq!(
        held(&d),
        vec![
            ("a knows b g2".into(), None),
            ("a likes c".into(), None),
            ("b knows c".into(), None),
        ]
    );
    Ok(())
}

#[test]
fn test_a_single_valued_insert_replaces_other_objects_and_keeps_a_rewritten_one() -> TestResult {
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
    )?;
    assert_eq!(
        d.statements()
            .next()
            .context("a statement")?
            .properties()
            .len(),
        1,
        "writing the value again keeps its statement"
    );
    apply(&mut d, vec![insert("a", "age", Literal::from(31))])?;
    let held: Vec<Statement> = d.statements().collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].object(), &Term::from(Literal::from(31)));
    Ok(())
}

#[test]
fn test_a_single_valued_write_replaces_only_its_subjects_object_in_its_graph() -> TestResult {
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
    )?;
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
    )?;
    assert_eq!(held(&d)[0], (format!("a age {}", typed(32)), Some(1)));
    assert_eq!(d.triple_count(), 5);
    Ok(())
}

/// A dataset holding `a knows b` as two statements.
fn held_twice() -> TestResult<CanonicalDataset> {
    let mut d = CanonicalDataset::new();
    apply(
        &mut d,
        vec![
            edge("a", "knows", "b", vec![]),
            edge("a", "knows", "b", vec![]),
        ],
    )?;
    Ok(d)
}

/// A triple term is refused, as an insert's object and as an edge's, named as oxrdf writes the triple.
fn assert_triple_terms_refused(d: &mut CanonicalDataset) -> TestResult {
    let triple = Triple::new(node("a"), iri("p"), iri("b"));
    let refused = MappingError::unsupported_object(triple.to_string());
    let insert_error = refusal(
        d,
        vec![insert("a", "says", Term::Triple(Box::new(triple.clone())))],
    )?;
    assert_eq!(insert_error, refused);
    let edge_error = refusal(
        d,
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
    )?;
    assert_eq!(edge_error, refused);
    Ok(())
}

#[test]
fn test_a_refused_change_leaves_nothing_of_its_changeset() -> TestResult {
    let mut d = held_twice()?;
    let before = d.clone();
    let ambiguous = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("knows"),
        object: iri("b").into(),
    };
    let ambiguous_error = refusal(
        &mut d,
        vec![
            insert("x", "p", iri("y")),
            Change::SetProperty {
                statement: ambiguous,
                predicate: iri("w"),
                value: Literal::from(1).into(),
            },
        ],
    )?;
    assert_eq!(
        ambiguous_error,
        MappingError::ambiguous_statement(
            "<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b>".into()
        )
    );
    assert_eq!(d, before);
    let missing = StatementRef::Reifier {
        graph: G,
        reifier: model_reifier(99),
    };
    let missing_error = refusal(
        &mut d,
        vec![Change::DeleteProperty {
            statement: missing,
            predicate: iri("w"),
            value: None,
        }],
    )?;
    assert_eq!(missing_error, no_statement(99, "DEFAULT"));
    assert_eq!(d, before);
    let unheld = StatementRef::Triple {
        graph: G,
        subject: node("a"),
        predicate: iri("knows"),
        object: iri("z").into(),
    };
    let unheld_error = refusal(
        &mut d,
        vec![
            insert("x", "p", iri("y")),
            Change::SetProperty {
                statement: unheld,
                predicate: iri("w"),
                value: Literal::from(1).into(),
            },
        ],
    )?;
    assert_eq!(
        unheld_error,
        MappingError::no_such_statement(
            "<http://ex.org/a> <http://ex.org/knows> <http://ex.org/z>".into()
        )
    );
    assert_eq!(d, before);
    assert_triple_terms_refused(&mut d)?;
    assert_eq!(d, before, "no statement and no ID of a refused changeset");
    Ok(())
}

#[test]
fn test_declarations_change_no_statement() -> TestResult {
    let mut d = CanonicalDataset::new();
    let declaration = ultragraphdb_core_mapping::Declaration::Predicate(
        ultragraphdb_core_mapping::PredicateDeclaration::new(iri("p")),
    );
    apply(&mut d, vec![Change::Declare(declaration)])?;
    assert_eq!(d, CanonicalDataset::new());
    Ok(())
}
