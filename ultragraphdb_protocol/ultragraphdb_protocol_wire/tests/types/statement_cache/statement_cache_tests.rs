/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, ErrorCode, Execute, ExecuteView, PatternSlot, Prepare,
    PrepareView, QuadPattern, StatementCache, StatementError,
};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{frame_of, golden};

const SCHEMA: u64 = 0x5C4E_3A2B_1D0F_7E6A;

fn prepare_frame(pattern: QuadPattern<String>, types: Vec<ColumnType>) -> Vec<u8> {
    Prepare::new(4, 0, 0, pattern, types, Vec::new())
        .frame(golden("prepare").header())
        .unwrap()
}

fn execute_frame(statement: u64, parameters: &[&str]) -> Vec<u8> {
    Execute::new(
        statement,
        0,
        parameters.iter().map(|p| p.to_string()).collect(),
        Vec::new(),
    )
    .frame(golden("execute").header())
    .unwrap()
}

fn prepare(cache: &mut StatementCache, schema: u64) -> Result<Prepare, StatementError> {
    let bytes = prepare_frame(pattern(), vec![ColumnType::Node]);
    cache.prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), schema)
}

fn execute(
    cache: &StatementCache,
    statement: u64,
    parameters: &[&str],
    schema: u64,
) -> Result<QuadPattern<String>, StatementError> {
    let bytes = execute_frame(statement, parameters);
    cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), schema)
}

#[test]
fn test_a_prepare_is_answered_with_its_id_schema_and_types() {
    let mut cache = StatementCache::new(8);
    assert!(cache.is_empty());
    let reply = prepare(&mut cache, SCHEMA).unwrap();
    assert_eq!(
        reply,
        Prepare::new(
            4,
            1,
            SCHEMA,
            pattern(),
            vec![ColumnType::Node],
            vec![ColumnType::Term]
        )
    );
    assert_eq!(prepare(&mut cache, SCHEMA).unwrap().statement(), 2);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.namespace(1), Some(4));
    assert_eq!(cache.namespace(3), None);
    // The golden request answered against the golden reply's schema id builds the golden reply but its namespace
    // and stream, which are the request's.
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
    let all_unbound = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(all_unbound, Vec::new());
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
    // A term where the subject takes a node.
    let bytes = prepare_frame(pattern(), vec![ColumnType::Term]);
    let refused = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
        .unwrap_err();
    assert_eq!(
        refused,
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node)
    );
    assert!(cache.is_empty());
    // A node where the object takes any term is valid.
    let at_object = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound("<p>".to_string()),
        PatternSlot::Parameter(0),
    );
    let bytes = prepare_frame(at_object, vec![ColumnType::Node]);
    assert!(
        cache
            .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), 1)
            .is_ok()
    );
}

#[test]
fn test_an_execution_binds_its_parameters_by_position() {
    let mut cache = StatementCache::new(8);
    prepare(&mut cache, SCHEMA).unwrap();
    let bound = execute(&cache, 1, &["<http://ex.org/a>"], SCHEMA).unwrap();
    assert_eq!(
        bound,
        QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Bound("<http://ex.org/a>".to_string()),
            PatternSlot::Bound("<http://ex.org/name>".to_string()),
            PatternSlot::Unbound,
        )
    );
    // The golden Execute against it.
    let bytes = golden("execute").bytes();
    assert_eq!(
        cache.execute(&ExecuteView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA),
        Ok(bound)
    );
}

#[test]
fn test_a_stale_statement_is_refused_and_prepares_again() {
    let mut cache = StatementCache::new(8);
    prepare(&mut cache, SCHEMA).unwrap();
    assert_eq!(
        execute(&cache, 1, &["<a>"], SCHEMA + 1),
        Err(StatementError::Stale(1, SCHEMA, SCHEMA + 1))
    );
    let again = prepare(&mut cache, SCHEMA + 1).unwrap();
    assert_eq!(again.statement(), 2);
    assert_eq!(again.schema(), SCHEMA + 1);
    assert!(execute(&cache, 2, &["<a>"], SCHEMA + 1).is_ok());
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
        execute(&cache, 9, &["<a>"], SCHEMA),
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
        execute(&cache, 1, &["<a>", "<b>"], SCHEMA),
        Err(StatementError::Parameters(1, 1, 2))
    );
    // Stale before the parameter count.
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
        execute(&cache, 1, &["<a>"], SCHEMA),
        Err(StatementError::Unknown(1))
    );
    assert!(execute(&cache, 2, &["<a>"], SCHEMA).is_ok());
    assert!(execute(&cache, 3, &["<a>"], SCHEMA).is_ok());
    // Ids are not reused: the next is 4.
    assert_eq!(prepare(&mut cache, SCHEMA).unwrap().statement(), 4);
    // A capacity of zero holds one statement.
    let mut one = StatementCache::new(0);
    prepare(&mut one, SCHEMA).unwrap();
    prepare(&mut one, SCHEMA).unwrap();
    assert_eq!(one.len(), 1);
}

#[test]
fn test_a_statement_without_parameters_executes_with_none() {
    let mut cache = StatementCache::new(4);
    let bound = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound("<http://ex.org/a>".to_string()),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let bytes = prepare_frame(bound.clone(), Vec::new());
    let id = cache
        .prepare(&PrepareView::from_frame(&frame_of(&bytes)).unwrap(), SCHEMA)
        .unwrap()
        .statement();
    assert_eq!(execute(&cache, id, &[], SCHEMA), Ok(bound));
    assert_eq!(
        execute(&cache, id, &["<b>"], SCHEMA),
        Err(StatementError::Parameters(id, 0, 1))
    );
}
