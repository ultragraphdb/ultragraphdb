/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, ErrorCode, Execute, ExecuteView, PatternSlot, Prepare,
    PrepareView, QuadPattern, RequestTerm, StatementCache, StatementError, TermTable,
};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{frame_of, golden};

const SCHEMA: u64 = 0x5C4E_3A2B_1D0F_7E6A;

fn prepare_frame(terms: TermTable, pattern: QuadPattern, types: Vec<ColumnType>) -> Vec<u8> {
    Prepare::new(4, 0, 0, terms, pattern, types, Vec::new())
        .frame(golden("prepare").header())
        .unwrap()
}

/// An Execute of `statement` binding `parameters`, IRIs by text, in a table of their own.
fn execute_frame(statement: u64, parameters: &[&str]) -> Vec<u8> {
    let mut terms = TermTable::new();
    let indices = parameters.iter().map(|p| terms.iri(p)).collect();
    Execute::new(statement, 5, 0, terms, indices, Vec::new())
        .frame(golden("execute").header())
        .unwrap()
}

fn prepare(cache: &mut StatementCache, schema: u64) -> Result<Prepare, StatementError> {
    let (terms, p) = pattern();
    let bytes = prepare_frame(terms, p, vec![ColumnType::Node]);
    cache.prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), schema)
}

fn execute(
    cache: &StatementCache,
    statement: u64,
    parameters: &[&str],
    schema: u64,
) -> Result<(TermTable, QuadPattern), StatementError> {
    let bytes = execute_frame(statement, parameters);
    cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), schema)
}

#[test]
fn test_a_prepare_is_answered_with_its_id_schema_and_types() {
    let mut cache = StatementCache::new(8);
    assert!(cache.is_empty());
    let (terms, p) = pattern();
    assert_eq!(
        prepare(&mut cache, SCHEMA).unwrap(),
        Prepare::new(
            4,
            1,
            SCHEMA,
            terms,
            p,
            vec![ColumnType::Node],
            vec![ColumnType::Term]
        )
    );
    assert_eq!(prepare(&mut cache, SCHEMA).unwrap().statement(), 2);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.namespace(1), Some(4));
    assert_eq!(cache.namespace(3), None);
    let request = golden("prepare").bytes();
    let mut fresh = StatementCache::new(8);
    let answered = fresh
        .prepare(
            &PrepareView::from_frame(&frame_of(&request)).unwrap(),
            SCHEMA,
        )
        .unwrap();
    assert_eq!(
        answered.frame(golden("prepare_reply").header()).unwrap(),
        golden("prepare_reply").bytes()
    );
}

#[test]
fn test_result_types_follow_the_unbound_slots() {
    let mut cache = StatementCache::new(8);
    let all = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(TermTable::new(), all, Vec::new());
    let reply = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
        .unwrap();
    assert_eq!(
        reply.result_types(),
        &[
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Term
        ]
    );
}

#[test]
fn test_a_parameter_type_its_slot_does_not_take_is_refused() {
    let mut cache = StatementCache::new(8);
    let (terms, p) = pattern();
    let bytes = prepare_frame(terms, p, vec![ColumnType::Term]);
    let refused = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
        .unwrap_err();
    assert_eq!(
        refused,
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node)
    );
    assert!(cache.is_empty());
    let mut terms = TermTable::new();
    let pred = terms.iri("p");
    let at_object = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound(pred),
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(terms, at_object, vec![ColumnType::Node]);
    assert!(
        cache
            .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
            .is_ok()
    );
}

#[test]
fn test_an_execution_binds_its_parameters_into_one_table() {
    let mut cache = StatementCache::new(8);
    prepare(&mut cache, SCHEMA).unwrap();
    let (terms, bound) = execute(&cache, 1, &["http://ex.org/a"], SCHEMA).unwrap();
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
    let bytes = golden("execute").bytes();
    assert_eq!(
        cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA),
        Ok((terms, bound))
    );
    // A parameter the statement already names is not added again.
    let (terms, bound) = execute(&cache, 1, &["http://ex.org/name"], SCHEMA).unwrap();
    assert_eq!(terms.len(), 1);
    assert_eq!(bound.subject(), PatternSlot::Bound(0));
}

#[test]
fn test_a_triple_term_parameter_brings_its_parts() {
    let mut cache = StatementCache::new(8);
    let all = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(TermTable::new(), all, vec![ColumnType::Term]);
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
        .unwrap()
        .statement();
    let mut terms = TermTable::new();
    let (filler, a, k, b) = (
        terms.iri("filler"),
        terms.iri("a"),
        terms.iri("k"),
        terms.iri("b"),
    );
    let triple = terms
        .intern(RequestTerm::Triple {
            subject: a,
            predicate: k,
            object: b,
        })
        .unwrap();
    let _ = filler;
    let execute = Execute::new(id, 5, 0, terms, vec![triple], Vec::new())
        .frame(golden("execute").header())
        .unwrap();
    let (bound_terms, bound) = cache
        .execute(&ExecuteView::from_frame(&frame_of(&execute)).unwrap(), 1)
        .unwrap();
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
}

#[test]
fn test_a_stale_statement_is_refused_and_prepares_again() {
    let mut cache = StatementCache::new(8);
    prepare(&mut cache, SCHEMA).unwrap();
    assert_eq!(
        execute(&cache, 1, &["a"], SCHEMA + 1),
        Err(StatementError::Stale(1, SCHEMA, SCHEMA + 1))
    );
    let again = prepare(&mut cache, SCHEMA + 1).unwrap();
    assert_eq!((again.statement(), again.schema()), (2, SCHEMA + 1));
    assert!(execute(&cache, 2, &["a"], SCHEMA + 1).is_ok());
    assert_eq!(
        StatementError::Stale(1, SCHEMA, SCHEMA + 1).code(),
        ErrorCode::StaleStatement
    );
}

#[test]
fn test_unknown_statements_and_wrong_parameter_counts_are_refused_in_that_order() {
    let mut cache = StatementCache::new(8);
    prepare(&mut cache, SCHEMA).unwrap();
    assert_eq!(
        execute(&cache, 9, &["a"], SCHEMA),
        Err(StatementError::Unknown(9))
    );
    assert_eq!(
        execute(&cache, 0, &[], SCHEMA),
        Err(StatementError::Unknown(0))
    );
    assert_eq!(
        execute(&cache, 1, &[], SCHEMA),
        Err(StatementError::Parameters(1, 1, 0))
    );
    assert_eq!(
        execute(&cache, 1, &["a", "b"], SCHEMA),
        Err(StatementError::Parameters(1, 1, 2))
    );
    assert_eq!(
        execute(&cache, 1, &[], SCHEMA + 1),
        Err(StatementError::Stale(1, SCHEMA, SCHEMA + 1))
    );
}

#[test]
fn test_past_its_capacity_the_oldest_statement_is_evicted() {
    let mut cache = StatementCache::new(2);
    for _ in 0..3 {
        prepare(&mut cache, SCHEMA).unwrap();
    }
    assert_eq!(cache.len(), 2);
    assert_eq!(
        execute(&cache, 1, &["a"], SCHEMA),
        Err(StatementError::Unknown(1))
    );
    assert!(execute(&cache, 2, &["a"], SCHEMA).is_ok());
    assert!(execute(&cache, 3, &["a"], SCHEMA).is_ok());
    assert_eq!(prepare(&mut cache, SCHEMA).unwrap().statement(), 4);
    let mut one = StatementCache::new(0);
    prepare(&mut one, SCHEMA).unwrap();
    prepare(&mut one, SCHEMA).unwrap();
    assert_eq!(one.len(), 1);
}

#[test]
fn test_a_statement_without_parameters_executes_with_none() {
    let mut cache = StatementCache::new(4);
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let bound = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(terms.clone(), bound, Vec::new());
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
        .unwrap()
        .statement();
    assert_eq!(execute(&cache, id, &[], SCHEMA), Ok((terms, bound)));
    assert_eq!(
        execute(&cache, id, &["b"], SCHEMA),
        Err(StatementError::Parameters(id, 0, 1))
    );
}

/// An Execute of `statement` binding `parameter`, a term of a table of its own.
fn execute_term(
    cache: &StatementCache,
    statement: u64,
    parameter: RequestTerm,
) -> Result<(), StatementError> {
    let mut terms = TermTable::new();
    let index = terms.intern(parameter).unwrap();
    let bytes = Execute::new(statement, 5, 0, terms, vec![index], Vec::new())
        .frame(golden("execute").header())
        .unwrap();
    cache
        .execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
        .map(|_| ())
}

#[test]
fn test_a_literal_or_a_triple_term_at_a_node_slot_is_refused() {
    // The golden statement's parameter is its subject, a slot taking a node.
    let mut cache = StatementCache::new(8);
    let id = prepare(&mut cache, SCHEMA).unwrap().statement();
    let literal = RequestTerm::Literal {
        lexical: "x".into(),
        datatype: None,
    };
    assert_eq!(
        execute_term(&cache, id, literal),
        Err(StatementError::NotANode(id, 0))
    );
    let tagged = RequestTerm::LanguageLiteral {
        lexical: "x".into(),
        language: "en".into(),
        direction: None,
    };
    assert_eq!(
        execute_term(&cache, id, tagged),
        Err(StatementError::NotANode(id, 0))
    );
    // A triple term, its parts before it in the execution's table.
    let mut terms = TermTable::new();
    let (a, k) = (terms.iri("a"), terms.iri("k"));
    let triple = terms
        .intern(RequestTerm::Triple {
            subject: a,
            predicate: k,
            object: a,
        })
        .unwrap();
    let bytes = Execute::new(id, 5, 0, terms, vec![triple], Vec::new())
        .frame(golden("execute").header())
        .unwrap();
    assert_eq!(
        cache
            .execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
            .map(|_| ()),
        Err(StatementError::NotANode(id, 0))
    );
    // An IRI and a blank node are nodes.
    assert_eq!(
        execute_term(&cache, id, RequestTerm::Iri("http://ex.org/a".into())),
        Ok(())
    );
    assert_eq!(
        execute_term(&cache, id, RequestTerm::BlankNode("b".into())),
        Ok(())
    );
    // The refusal is code 11, its detail the statement.
    let reply = StatementError::NotANode(id, 0).reply();
    assert_eq!(reply.code(), ErrorCode::Parameters);
    assert_eq!(reply.detail(), id);
}

#[test]
fn test_a_literal_at_a_slot_taking_any_term_executes() {
    let mut cache = StatementCache::new(8);
    let object = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(TermTable::new(), object, vec![ColumnType::Term]);
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
        .unwrap()
        .statement();
    let literal = RequestTerm::Literal {
        lexical: "x".into(),
        datatype: None,
    };
    assert_eq!(execute_term(&cache, id, literal), Ok(()));
}

#[test]
fn test_a_wrong_parameter_count_is_named_before_a_literal() {
    let mut cache = StatementCache::new(8);
    let id = prepare(&mut cache, SCHEMA).unwrap().statement();
    let mut terms = TermTable::new();
    let literal = terms
        .intern(RequestTerm::Literal {
            lexical: "x".into(),
            datatype: None,
        })
        .unwrap();
    let bytes = Execute::new(id, 5, 0, terms, vec![literal, literal], Vec::new())
        .frame(golden("execute").header())
        .unwrap();
    assert_eq!(
        cache
            .execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
            .map(|_| ()),
        Err(StatementError::Parameters(id, 1, 2))
    );
}
