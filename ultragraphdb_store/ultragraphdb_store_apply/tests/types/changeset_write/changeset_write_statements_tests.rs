/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Statement IDs and their properties: properties set and deleted by triple and by ID, one value or every value, in
//! the default graph and a named one; a triple several statements hold named only by an ID.

use oxrdf::GraphName;
use ultragraphdb_core_keys::DEFAULT_GRAPH;
use ultragraphdb_core_mapping::{Change, MappingErrorEnum, StatementId, StatementRef};
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_quads::StatementRecord;

use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets, ex, insert_edge, objects_in, postings, statement, term,
    uid, write_changes,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn by_triple(text: &str) -> TestResult<StatementRef> {
    let t = statement(text)?;
    Ok(StatementRef::Triple {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
    })
}

fn record(a: &TestChangesets, graph: u64, id: u64) -> TestResult<Option<StatementRecord>> {
    Ok(a.quads()
        .statement(NS, graph, id, a.quads().lists().applied_ts())?)
}

/// The properties of a record, as `(predicate name, value text)`.
fn properties(a: &TestChangesets, record: &StatementRecord) -> TestResult<Vec<(u64, String)>> {
    let mut p: Vec<(u64, String)> = record
        .properties()
        .iter()
        .map(|(p, v)| Ok((*p, a.quads().term_of(NS, v)?.to_string())))
        .collect::<TestResult<_>>()?;
    p.sort();
    Ok(p)
}

#[test]
fn test_a_property_is_deleted_by_value_or_entirely() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2020")])?],
    )?;
    let set = |p: &str, v: &str| -> TestResult<Change> {
        Ok(Change::SetProperty {
            statement: by_triple("<a> <knows> <b>")?,
            predicate: ex(p),
            value: term(v)?,
        })
    };
    let del = |p: &str, v: Option<&str>| -> TestResult<Change> {
        Ok(Change::DeleteProperty {
            statement: by_triple("<a> <knows> <b>")?,
            predicate: ex(p),
            value: v.map(term).transpose()?,
        })
    };
    // Setting a value held already adds nothing.
    write_changes(
        &a,
        vec![
            set("since", "2021")?,
            set("since", "2020")?,
            set("weight", "1")?,
        ],
    )?;
    let id = postings(&a, "a", "knows")?[0].1[0];
    let integer = |n: &str| format!("\"{n}\"^^<http://www.w3.org/2001/XMLSchema#integer>");
    let (since, weight) = (uid(&a, "since")?, uid(&a, "weight")?);
    let held = || {
        properties(
            &a,
            &record(&a, DEFAULT_GRAPH, id)?.context("the record is held")?,
        )
    };
    let mut all = vec![
        (since, integer("2020")),
        (since, integer("2021")),
        (weight, integer("1")),
    ];
    all.sort();
    assert_eq!(held()?, all);
    write_changes(&a, vec![del("since", Some("2020"))?])?;
    let mut left = vec![(since, integer("2021")), (weight, integer("1"))];
    left.sort();
    assert_eq!(held()?, left);
    // A value the statement does not hold, and a property never written, change nothing.
    write_changes(&a, vec![del("since", Some("1999"))?, del("never", None)?])?;
    assert_eq!(held()?, left);
    write_changes(&a, vec![del("since", None)?])?;
    assert_eq!(held()?, vec![(weight, integer("1"))]);
    // The statement itself stays.
    assert_eq!(postings(&a, "a", "knows")?[0].1, vec![id]);
    Ok(())
}

#[test]
fn test_a_triple_held_by_two_statements_is_named_by_an_id() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2020")])?],
    )?;
    write_changes(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2021")])?],
    )?;
    let refused = apply(
        &a,
        vec![Change::SetProperty {
            statement: by_triple("<a> <knows> <b>")?,
            predicate: ex("weight"),
            value: term("1")?,
        }],
    )
    .err()
    .context("the error of apply(..)")?;
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Mapping(e) if matches!(e.0, MappingErrorEnum::AmbiguousStatement { .. })),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b> is held by several statements; name one by its statement ID"
    );
    let ids = postings(&a, "a", "knows")?[0].1.clone();
    write_changes(
        &a,
        vec![Change::SetProperty {
            statement: StatementRef::Id {
                graph: GraphName::DefaultGraph,
                id: StatementId::new(ids[1]),
            },
            predicate: ex("weight"),
            value: term("1")?,
        }],
    )?;
    assert_eq!(
        record(&a, DEFAULT_GRAPH, ids[1])?
            .context("record(&a, DEFAULT_GRAPH, ids[1])?")?
            .properties()
            .len(),
        2
    );
    assert_eq!(
        record(&a, DEFAULT_GRAPH, ids[0])?
            .context("record(&a, DEFAULT_GRAPH, ids[0])?")?
            .properties()
            .len(),
        1
    );
    Ok(())
}

#[test]
fn test_an_id_no_statement_has_is_refused_or_changes_nothing() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2020")])?],
    )?;
    let by_id = StatementRef::Id {
        graph: GraphName::DefaultGraph,
        id: StatementId::new(99),
    };
    let refused = apply(
        &a,
        vec![Change::SetProperty {
            statement: by_id,
            predicate: ex("weight"),
            value: term("1")?,
        }],
    )
    .err()
    .context("the error of apply(..)")?;
    assert_eq!(refused.to_string(), "No statement 99 in DEFAULT is held");
    let before = a.quads().store().contents();
    write_changes(
        &a,
        vec![Change::DeleteStatement {
            graph: GraphName::DefaultGraph,
            id: StatementId::new(99),
        }],
    )?;
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    Ok(())
}

#[test]
fn test_statements_in_a_named_graph_keep_their_records_there() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![insert_edge("<a> <knows> <b> <g1>", &[("since", "2020")])?],
    )?;
    let g1 = uid(&a, "g1")?;
    let key = ultragraphdb_core_keys::QuadKey::Forward {
        namespace: NS,
        graph: g1,
        predicate: uid(&a, "knows")?,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    let ids = a.quads().objects(&key, a.quads().lists().applied_ts())?[0]
        .1
        .clone();
    assert_eq!(ids.len(), 1);
    assert!(record(&a, g1, ids[0])?.is_some());
    assert!(record(&a, DEFAULT_GRAPH, ids[0])?.is_none());
    write_changes(
        &a,
        vec![Change::DeleteStatement {
            graph: GraphName::from(ex("g1")),
            id: StatementId::new(ids[0]),
        }],
    )?;
    assert!(record(&a, g1, ids[0])?.is_none());
    assert!(objects_in(&a, "g1", "a", "knows")?.is_empty());
    Ok(())
}
