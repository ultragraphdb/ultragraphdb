/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a client's Read asks, read once as it arrives (`PROTOCOL.md` §4.4): a slot holding a term of the wrong kind is refused
//! with code 7, `detail` the term's index.

use ultragraphdb_common_compute::ReadPattern;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, FrameView, Header, MAX_FRAME_SIZE, PatternSlot,
    QuadPattern, Read, ReadView, RequestTerm, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::CLUSTER;

/// What `ReadPattern::of_read` makes of a Read of `terms` and `pattern`.
fn of(terms: TermTable, pattern: QuadPattern) -> TestResult<Result<ReadPattern, (ErrorCode, u64)>> {
    let frame = Read::new(1, 0, terms, pattern, Vec::new())
        .frame(Header::new(CLUSTER, Command::Read).with_stream(3))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(ReadPattern::of_read(&ReadView::from_frame(&view)?)
        .map_err(|refusal| (refusal.code(), refusal.detail())))
}

#[test]
fn test_iris_and_a_literal_object_are_taken() -> TestResult {
    let mut terms = TermTable::new();
    let (g, s, p) = (
        terms.iri("http://g"),
        terms.iri("http://s"),
        terms.iri("http://p"),
    );
    let o = terms.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    let pattern = QuadPattern::new(
        PatternSlot::Bound(g),
        PatternSlot::Bound(s),
        PatternSlot::Bound(p),
        PatternSlot::Bound(o),
    );
    assert!(of(terms, pattern)?.is_ok());
    Ok(())
}

/// A literal in the subject slot, the term of index 1, is refused with code 7, `detail` 1.
#[test]
fn test_a_literal_subject_is_refused_naming_its_index() -> TestResult {
    let mut terms = TermTable::new();
    let p = terms.iri("http://p");
    let s = terms.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Bound(s),
        PatternSlot::Bound(p),
        PatternSlot::Unbound,
    );
    let refused = of(terms, pattern)?.err().context("a refusal")?;
    assert_eq!(refused, (ErrorCode::Refused, 1));
    Ok(())
}

/// A blank node's label in the object slot, the term of index 1, is refused with code 7, `detail` 1.
#[test]
fn test_a_blank_node_object_is_refused_naming_its_index() -> TestResult {
    let mut terms = TermTable::new();
    let s = terms.iri("http://s");
    let o = terms.intern(RequestTerm::BlankNode("b".to_string()))?;
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Bound(s),
        PatternSlot::Unbound,
        PatternSlot::Bound(o),
    );
    let refused = of(terms, pattern)?.err().context("a refusal")?;
    assert_eq!(refused, (ErrorCode::Refused, 1));
    Ok(())
}
