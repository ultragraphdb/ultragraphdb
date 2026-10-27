/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Structure-aware: a request's term table built from arbitrary terms and a triple-term chain of an arbitrary depth,
//! at and past the nesting bound, carried by an Execute. A table the builder accepts reads back entry for entry and
//! nests at most `MAX_TERM_DEPTH`; a chain one deeper is refused. Then the records and text are mutated, the frame
//! rebuilt around them, and the view reads or refuses them without a panic, never accepting a term deeper than the
//! bound.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Execute, ExecuteView, FrameView, Header, MAX_TERM_DEPTH,
    RequestTerm, RequestTermView, TermTable,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

#[derive(Debug, Arbitrary)]
enum Term {
    Iri(String),
    BlankNode(String),
    Literal(String, Option<String>),
    LanguageLiteral(String, String),
    Triple(u16, u16, u16),
}

#[derive(Debug, Arbitrary)]
struct Input {
    /// A chain of triple terms over the first entry, each naming the one before at the place `chain_part % 3`. First,
    /// so a seed's first byte sets the depth: `[32, 2]` is a chain at the bound, `[33, 0]` one past it.
    chain: u8,
    chain_part: u8,
    terms: Vec<Term>,
    /// Byte flips of the body: offset and bit.
    flips: Vec<(u16, u8)>,
}

fuzz_target!(|input: Input| {
    let mut table = TermTable::new();
    let first = table.iri("http://ex.org/a");
    for term in &input.terms {
        let len = table.len() as u32;
        let term = match term {
            Term::Iri(s) => RequestTerm::Iri(s.clone()),
            Term::BlankNode(s) => RequestTerm::BlankNode(s.clone()),
            Term::Literal(lexical, datatype) => RequestTerm::Literal {
                lexical: lexical.clone(),
                datatype: datatype.clone(),
            },
            Term::LanguageLiteral(lexical, language) => RequestTerm::LanguageLiteral {
                lexical: lexical.clone(),
                language: language.clone(),
                direction: None,
            },
            Term::Triple(s, p, o) => RequestTerm::Triple {
                subject: u32::from(*s) % len,
                predicate: u32::from(*p) % len,
                object: u32::from(*o) % len,
            },
        };
        let _ = table.intern(term);
    }
    // The chain: its k-th link is depth k; the builder refuses the link past the bound.
    let mut deepest = first;
    for k in 1..=usize::from(input.chain % 40) {
        let mut parts = [first; 3];
        parts[usize::from(input.chain_part % 3)] = deepest;
        let link = RequestTerm::Triple {
            subject: parts[0],
            predicate: parts[1],
            object: parts[2],
        };
        match table.intern(link) {
            Ok(index) => deepest = index,
            Err(refusal) => {
                assert!(k > MAX_TERM_DEPTH, "a chain {k} deep is refused: {refusal}");
                assert!(matches!(refusal.0, ProtocolErrorEnum::TermTooDeep { .. }));
                break;
            }
        }
    }
    let parameters = vec![deepest];
    let execute = Execute::new(1, 0, table.clone(), parameters, Vec::new());
    let Ok(frame) = execute.frame(Header::new(CLUSTER, Command::Execute)) else {
        return;
    };
    let view = FrameView::validate(&frame, CLUSTER).expect("a built frame validates");
    let read = ExecuteView::from_frame(&view).expect("a built table reads");
    assert_eq!(read.terms().to_table(), table);
    for (i, term) in table.terms().iter().enumerate() {
        assert_eq!(
            read.terms().get(i as u32).map(|t| t.to_term()).as_ref(),
            Some(term)
        );
    }
    // Mutated: read or refused, never a panic; an accepted table nests within the bound.
    let mut body = view.body().to_vec();
    for (at, bit) in &input.flips {
        if let Some(byte) = body.get_mut(usize::from(*at)) {
            *byte ^= 1 << (bit % 8);
        }
    }
    let Ok(mutated) = Header::new(CLUSTER, Command::Execute).encode_frame(&body) else {
        return;
    };
    let view = FrameView::validate(&mutated, CLUSTER).expect("a rebuilt frame validates");
    if let Ok(read) = ExecuteView::from_frame(&view) {
        let terms = read.terms();
        let mut depths: Vec<usize> = Vec::new();
        for i in 0..terms.len() as u32 {
            let d = match terms.get(i) {
                Some(RequestTermView::Triple {
                    subject,
                    predicate,
                    object,
                }) => {
                    1 + [subject, predicate, object]
                        .iter()
                        .map(|p| depths.get(*p as usize).copied().unwrap_or(0))
                        .max()
                        .unwrap_or(0)
                }
                _ => 0,
            };
            assert!(d <= MAX_TERM_DEPTH, "an accepted term nests {d} deep");
            depths.push(d);
        }
    }
});
