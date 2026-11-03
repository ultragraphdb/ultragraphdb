/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, ErrorCode, Execute, ExecuteView, PatternSlot, Prepare,
    PrepareView, QuadPattern, RequestTerm, StatementCache, StatementError, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{frame_of, golden};

const SCHEMA: u64 = 0x5C4E_3A2B_1D0F_7E6A;

fn prepare_frame(
    terms: TermTable,
    pattern: QuadPattern,
    types: Vec<ColumnType>,
) -> TestResult<Vec<u8>> {
    Ok(Prepare::new(4, 0, 0, terms, pattern, types, Vec::new())
        .frame(golden("prepare")?.header())?)
}

/// An Execute of `statement` binding `parameters`, IRIs by text, in a table of their own.
fn execute_frame(statement: u64, parameters: &[&str]) -> TestResult<Vec<u8>> {
    let mut terms = TermTable::new();
    let indices = parameters.iter().map(|p| terms.iri(p)).collect();
    Ok(Execute::new(statement, 5, 0, terms, indices, Vec::new())
        .frame(golden("execute")?.header())?)
}

/// The cache's answer to a Prepare of the shared pattern; the outer result fails the test when the frame cannot be
/// built.
fn prepare(cache: &mut StatementCache, schema: u64) -> TestResult<Result<Prepare, StatementError>> {
    let (terms, p) = pattern();
    let bytes = prepare_frame(terms, p, vec![ColumnType::Node])?;
    Ok(cache.prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, schema))
}

fn execute(
    cache: &StatementCache,
    statement: u64,
    parameters: &[&str],
    schema: u64,
) -> TestResult<Result<(TermTable, QuadPattern), StatementError>> {
    let bytes = execute_frame(statement, parameters)?;
    Ok(cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)?)?, schema))
}

#[test]
fn test_a_prepare_is_answered_with_its_id_schema_and_types() -> TestResult {
    let mut cache = StatementCache::new(8);
    assert!(cache.is_empty());
    let (terms, p) = pattern();
    assert_eq!(
        prepare(&mut cache, SCHEMA)??,
        Prepare::new(
            4,
            1,
            SCHEMA,
            terms,
            p,
            vec![ColumnType::Node],
            vec![ColumnType::Term, ColumnType::Node]
        )
    );
    assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), 2);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.namespace(1), Some(4));
    assert_eq!(cache.namespace(3), None);
    let request = golden("prepare")?.bytes()?;
    let mut fresh = StatementCache::new(8);
    let answered = fresh.prepare(&PrepareView::from_frame(&frame_of(&request)?)?, SCHEMA)?;
    assert_eq!(
        answered.frame(golden("prepare_reply")?.header())?,
        golden("prepare_reply")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_result_types_follow_the_unbound_slots() -> TestResult {
    let mut cache = StatementCache::new(8);
    let all = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(TermTable::new(), all, Vec::new())?;
    let reply = cache.prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, 1)?;
    assert_eq!(
        reply.result_types(),
        &[
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Term,
            ColumnType::Node
        ]
    );
    Ok(())
}

#[test]
fn test_a_parameter_type_its_slot_does_not_take_is_refused() -> TestResult {
    let mut cache = StatementCache::new(8);
    let (terms, p) = pattern();
    let bytes = prepare_frame(terms, p, vec![ColumnType::Term])?;
    let refused = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, 1)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused,
        StatementError::parameter_type(0, ColumnType::Term, ColumnType::Node)
    );
    assert!(cache.is_empty());
    let mut object_terms = TermTable::new();
    let pred = object_terms.iri("p");
    let at_object = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound(pred),
        PatternSlot::Parameter(0),
    );
    let object_frame = prepare_frame(object_terms, at_object, vec![ColumnType::Node])?;
    // A node parameter at the object, which takes any term, is taken: the first statement prepared, id 1.
    assert_eq!(
        cache
            .prepare(&PrepareView::from_frame(&frame_of(&object_frame)?)?, 1)
            .map(|reply| reply.statement()),
        Ok(1)
    );
    Ok(())
}

#[test]
fn test_an_execution_binds_its_parameters_into_one_table() -> TestResult {
    let mut cache = StatementCache::new(8);
    assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), 1);
    let (terms, bound) = execute(&cache, 1, &["http://ex.org/a"], SCHEMA)??;
    // The statement's <name>, then the parameter <a>.
    assert_eq!(
        terms.terms(),
        &[
            RequestTerm::Iri("http://ex.org/name".to_string()),
            RequestTerm::Iri("http://ex.org/a".to_string())
        ]
    );
    assert_eq!(
        bound,
        QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Bound(1),
            PatternSlot::Bound(0),
            PatternSlot::Unbound
        )
    );
    let bytes = golden("execute")?.bytes()?;
    assert_eq!(
        cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)?)?, SCHEMA),
        Ok((terms, bound))
    );
    // A parameter the statement already names is not added again.
    let (named_terms, named_bound) = execute(&cache, 1, &["http://ex.org/name"], SCHEMA)??;
    assert_eq!(named_terms.len(), 1);
    assert_eq!(named_bound.subject(), PatternSlot::Bound(0));
    Ok(())
}

#[test]
fn test_a_triple_term_parameter_brings_its_parts() -> TestResult {
    let mut cache = StatementCache::new(8);
    let all = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(TermTable::new(), all, vec![ColumnType::Term])?;
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, 1)?
        .statement();
    let mut terms = TermTable::new();
    let (_filler, a, k, b) = (
        terms.iri("filler"),
        terms.iri("a"),
        terms.iri("k"),
        terms.iri("b"),
    );
    let triple = terms.intern(RequestTerm::Triple {
        subject: a,
        predicate: k,
        object: b,
    })?;
    let execute = Execute::new(id, 5, 0, terms, vec![triple], Vec::new())
        .frame(golden("execute")?.header())?;
    let (bound_terms, bound) = cache.execute(&ExecuteView::from_frame(&frame_of(&execute)?)?, 1)?;
    assert_eq!(bound_terms.len(), 4);
    assert_eq!(
        bound_terms.get(3),
        Some(&RequestTerm::Triple {
            subject: 0,
            predicate: 1,
            object: 2
        })
    );
    assert_eq!(bound.object(), PatternSlot::Bound(3));
    Ok(())
}

#[test]
fn test_a_parameter_naming_one_part_at_every_level_imports_each_entry_once() -> TestResult {
    // t0 = <a>, t(k+1) = <<( t(k) <p> t(k) )>> to the depth bound, 32: every triple names its part twice, so a walk of
    // every path would take 2^32 steps. The bound table holds each entry once, parts before the triple: <a>, <p>,
    // then t1 to t32, 34 terms.
    let mut cache = StatementCache::new(8);
    let all = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(TermTable::new(), all, vec![ColumnType::Term])?;
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, 1)?
        .statement();
    let mut terms = TermTable::new();
    let (_filler, p) = (terms.iri("filler"), terms.iri("p"));
    let mut deepest = terms.iri("a");
    for _ in 0..32 {
        deepest = terms.intern(RequestTerm::Triple {
            subject: deepest,
            predicate: p,
            object: deepest,
        })?;
    }
    let execute = Execute::new(id, 5, 0, terms, vec![deepest], Vec::new())
        .frame(golden("execute")?.header())?;
    let (bound_terms, bound) = cache.execute(&ExecuteView::from_frame(&frame_of(&execute)?)?, 1)?;
    assert_eq!(bound_terms.len(), 34);
    // The subject's part first: <a>, then <p>, then t1.
    assert_eq!(bound_terms.get(0), Some(&RequestTerm::Iri("a".to_string())));
    assert_eq!(bound_terms.get(1), Some(&RequestTerm::Iri("p".to_string())));
    assert_eq!(
        bound_terms.get(2),
        Some(&RequestTerm::Triple {
            subject: 0,
            predicate: 1,
            object: 0
        })
    );
    assert_eq!(
        bound_terms.get(33),
        Some(&RequestTerm::Triple {
            subject: 32,
            predicate: 1,
            object: 32
        })
    );
    assert_eq!(bound.object(), PatternSlot::Bound(33));
    Ok(())
}

#[test]
fn test_a_stale_statement_is_refused_and_prepares_again() -> TestResult {
    let mut cache = StatementCache::new(8);
    assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), 1);
    assert_eq!(
        execute(&cache, 1, &["a"], SCHEMA + 1)?,
        Err(StatementError::stale(1, SCHEMA, SCHEMA + 1))
    );
    let again = prepare(&mut cache, SCHEMA + 1)??;
    assert_eq!((again.statement(), again.schema()), (2, SCHEMA + 1));
    // The parameter <a> follows the statement's <name>: index 1, at the subject.
    assert_eq!(
        execute(&cache, 2, &["a"], SCHEMA + 1)?.map(|(_, p)| p.subject()),
        Ok(PatternSlot::Bound(1))
    );
    assert_eq!(
        StatementError::stale(1, SCHEMA, SCHEMA + 1).code(),
        ErrorCode::StaleStatement
    );
    Ok(())
}

#[test]
fn test_unknown_statements_and_wrong_parameter_counts_are_refused_in_that_order() -> TestResult {
    let mut cache = StatementCache::new(8);
    assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), 1);
    assert_eq!(
        execute(&cache, 9, &["a"], SCHEMA)?,
        Err(StatementError::unknown(9))
    );
    assert_eq!(
        execute(&cache, 0, &[], SCHEMA)?,
        Err(StatementError::unknown(0))
    );
    assert_eq!(
        execute(&cache, 1, &[], SCHEMA)?,
        Err(StatementError::parameters(1, 1, 0))
    );
    assert_eq!(
        execute(&cache, 1, &["a", "b"], SCHEMA)?,
        Err(StatementError::parameters(1, 1, 2))
    );
    assert_eq!(
        execute(&cache, 1, &[], SCHEMA + 1)?,
        Err(StatementError::stale(1, SCHEMA, SCHEMA + 1))
    );
    Ok(())
}

#[test]
fn test_past_its_capacity_the_oldest_statement_is_evicted() -> TestResult {
    let mut cache = StatementCache::new(2);
    for expected in 1..=3 {
        assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), expected);
    }
    assert_eq!(cache.len(), 2);
    assert_eq!(
        execute(&cache, 1, &["a"], SCHEMA)?,
        Err(StatementError::unknown(1))
    );
    for statement in [2, 3] {
        assert_eq!(
            execute(&cache, statement, &["a"], SCHEMA)?.map(|(_, p)| p.subject()),
            Ok(PatternSlot::Bound(1)),
            "statement {statement}"
        );
    }
    assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), 4);
    let mut one = StatementCache::new(0);
    assert_eq!(prepare(&mut one, SCHEMA)??.statement(), 1);
    assert_eq!(prepare(&mut one, SCHEMA)??.statement(), 2);
    assert_eq!(one.len(), 1);
    Ok(())
}

#[test]
fn test_a_statement_without_parameters_executes_with_none() -> TestResult {
    let mut cache = StatementCache::new(4);
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let bound = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(terms.clone(), bound, Vec::new())?;
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, SCHEMA)?
        .statement();
    assert_eq!(execute(&cache, id, &[], SCHEMA)?, Ok((terms, bound)));
    assert_eq!(
        execute(&cache, id, &["b"], SCHEMA)?,
        Err(StatementError::parameters(id, 0, 1))
    );
    Ok(())
}

/// An Execute of `statement` binding `parameter`, a term of a table of its own.
fn execute_term(
    cache: &StatementCache,
    statement: u64,
    parameter: RequestTerm,
) -> TestResult<Result<(), StatementError>> {
    let mut terms = TermTable::new();
    let index = terms.intern(parameter)?;
    let bytes = Execute::new(statement, 5, 0, terms, vec![index], Vec::new())
        .frame(golden("execute")?.header())?;
    Ok(cache
        .execute(&ExecuteView::from_frame(&frame_of(&bytes)?)?, SCHEMA)
        .map(|_| ()))
}

#[test]
fn test_a_literal_or_a_triple_term_at_a_node_slot_is_refused() -> TestResult {
    // The golden statement's parameter is its subject, a slot taking a node.
    let mut cache = StatementCache::new(8);
    let id = prepare(&mut cache, SCHEMA)??.statement();
    let literal = RequestTerm::Literal {
        lexical: "x".into(),
        datatype: None,
    };
    assert_eq!(
        execute_term(&cache, id, literal)?,
        Err(StatementError::not_a_node(id, 0))
    );
    let tagged = RequestTerm::LanguageLiteral {
        lexical: "x".into(),
        language: "en".into(),
        direction: None,
    };
    assert_eq!(
        execute_term(&cache, id, tagged)?,
        Err(StatementError::not_a_node(id, 0))
    );
    // A triple term, its parts before it in the execution's table.
    let mut terms = TermTable::new();
    let (a, k) = (terms.iri("a"), terms.iri("k"));
    let triple = terms.intern(RequestTerm::Triple {
        subject: a,
        predicate: k,
        object: a,
    })?;
    let bytes = Execute::new(id, 5, 0, terms, vec![triple], Vec::new())
        .frame(golden("execute")?.header())?;
    assert_eq!(
        cache
            .execute(&ExecuteView::from_frame(&frame_of(&bytes)?)?, SCHEMA)
            .map(|_| ()),
        Err(StatementError::not_a_node(id, 0))
    );
    // An IRI and a blank node are nodes.
    assert_eq!(
        execute_term(&cache, id, RequestTerm::Iri("http://ex.org/a".into()))?,
        Ok(())
    );
    assert_eq!(
        execute_term(&cache, id, RequestTerm::BlankNode("b".into()))?,
        Ok(())
    );
    // The refusal is code 11, its detail the statement.
    let reply = StatementError::not_a_node(id, 0).reply();
    assert_eq!(reply.code(), ErrorCode::Parameters);
    assert_eq!(reply.detail(), id);
    Ok(())
}

#[test]
fn test_a_literal_at_a_slot_taking_any_term_executes() -> TestResult {
    let mut cache = StatementCache::new(8);
    let object = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(TermTable::new(), object, vec![ColumnType::Term])?;
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)?)?, SCHEMA)?
        .statement();
    let literal = RequestTerm::Literal {
        lexical: "x".into(),
        datatype: None,
    };
    assert_eq!(execute_term(&cache, id, literal)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_wrong_parameter_count_is_named_before_a_literal() -> TestResult {
    let mut cache = StatementCache::new(8);
    let id = prepare(&mut cache, SCHEMA)??.statement();
    let mut terms = TermTable::new();
    let literal = terms.intern(RequestTerm::Literal {
        lexical: "x".into(),
        datatype: None,
    })?;
    let bytes = Execute::new(id, 5, 0, terms, vec![literal, literal], Vec::new())
        .frame(golden("execute")?.header())?;
    assert_eq!(
        cache
            .execute(&ExecuteView::from_frame(&frame_of(&bytes)?)?, SCHEMA)
            .map(|_| ()),
        Err(StatementError::parameters(id, 1, 2))
    );
    Ok(())
}

#[test]
fn test_the_default_cache_holds_256_statements() -> TestResult {
    // Point 8: section 11.9's default, the statements a connection holds.
    assert_eq!(StatementCache::DEFAULT_CAPACITY, 256);
    let mut cache = StatementCache::new(StatementCache::DEFAULT_CAPACITY);
    for expected in 1..=StatementCache::DEFAULT_CAPACITY as u64 + 1 {
        assert_eq!(prepare(&mut cache, SCHEMA)??.statement(), expected);
    }
    assert_eq!(cache.len(), 256);
    assert_eq!(
        execute(&cache, 1, &["a"], SCHEMA)?,
        Err(StatementError::unknown(1))
    );
    assert_eq!(
        execute(&cache, 2, &["a"], SCHEMA)?.map(|(_, p)| p.subject()),
        Ok(PatternSlot::Bound(1))
    );
    Ok(())
}
