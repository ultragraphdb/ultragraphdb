/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_terms::{Datatype, EncodedTerm, Hash128, TermError, TermView, Text};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Header, ResultBatch, ResultTerm, ResultView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::result_batch::result_batch_tests::{LONG, SKOLEM_6};
use crate::utils::golden_bodies::{CLUSTER, frame_of, golden};

/// A datatype no known-datatype code names, which a Result names by the hash of its IRI (`PROTOCOL.md` §10.1).
const DATATYPE: &str = "http://ex.org/celsius";
/// A lexical form short enough to be held inline.
const SHORT: &str = "21.5";

#[test]
fn test_a_term_reads_through_core_terms_view() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    let al = view.term(1).context("term 1 of the batch")?;
    assert_eq!(al.encoding(), &[0x20, 2, b'A', b'l']);
    assert_eq!(al.text(), None);
    assert_eq!(al.annotation(), None);
    assert_eq!(al.hash(), None);
    let term: EncodedTerm = al.term().context("the term in the store's encoding")?;
    assert_eq!(term.view()?, TermView::String(Text::Inline("Al")));
    let a = view.term(0).context("term 0 of the batch")?;
    assert_eq!(a.term(), None);
    assert_eq!(a.hash(), Some(Hash128::of(b"http://ex.org/a")));
    assert_eq!(a.text(), Some("http://ex.org/a"));
    assert!(!a.is_blank_node());
    Ok(())
}

#[test]
fn test_a_blank_node_reads_as_its_skolem_iris_hash_with_its_own_tag() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    let blank = view.term(4).context("term 4 of the batch")?;
    assert!(blank.is_blank_node());
    let hash = Hash128::of(SKOLEM_6.as_bytes());
    assert_eq!(blank.encoding().first(), Some(&0x02));
    assert_eq!(blank.encoding().get(1..), Some(&hash.as_bytes()[..]));
    assert_eq!(blank.hash(), Some(hash));
    assert_eq!(blank.term(), None);
    assert_eq!(blank.text(), Some(SKOLEM_6));
    Ok(())
}

/// A literal of a datatype no code names holds the hash of its datatype's IRI where the store holds an ID (`PROTOCOL.md`
/// §10.1): tag `0x32` with its lexical form inline, `0x33` with its hash. A client reads both back, the datatype by that
/// hash, which the term's annotation names, and the literal from the texts the term carries.
#[test]
fn test_a_literal_of_another_datatype_reads_back_with_its_datatype_by_hash() -> TestResult {
    let datatype = Hash128::of(DATATYPE.as_bytes());
    let short = [
        &[0x32][..],
        datatype.as_bytes(),
        &[SHORT.len() as u8],
        SHORT.as_bytes(),
    ]
    .concat();
    let long_hash = Hash128::of(LONG.as_bytes());
    let long = [&[0x33][..], datatype.as_bytes(), long_hash.as_bytes()].concat();
    let terms = [
        ResultTerm::new(EncodedTerm::from_bytes(&short)?).with_annotation(DATATYPE),
        ResultTerm::new(EncodedTerm::from_bytes(&long)?)
            .with_text(LONG)
            .with_annotation(DATATYPE),
    ];
    // Two unbound slots and the reifier column, absent.
    let mut batch = ResultBatch::new(3);
    batch.push_row(&[Some(&terms[0]), Some(&terms[1]), None])?;
    let bytes = batch.frame(Header::new(CLUSTER, Command::Result).with_stream(1))?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    for (index, lexical, text, form) in [
        (0, Text::Inline(SHORT), None, SHORT),
        (1, Text::Hashed(long_hash), Some(LONG), LONG),
    ] {
        let read = view.term(index).context("the literal's term")?;
        assert_eq!(read.text(), text);
        assert_eq!(read.annotation(), Some(DATATYPE));
        let term = read.term().context("the literal in a Result's encoding")?;
        let viewed = term.result_view()?;
        assert_eq!(
            viewed,
            TermView::Typed {
                datatype: Datatype::Hashed(datatype),
                lexical,
            }
        );
        // The texts a hash names: the term's text and its annotation.
        let named = |hash: Hash128| {
            [read.text(), read.annotation()]
                .into_iter()
                .flatten()
                .find(|t| Hash128::of(t.as_bytes()) == hash)
                .map(str::to_string)
                .ok_or(TermError::malformed(hash.as_bytes().to_vec(), "no text"))
        };
        assert_eq!(
            viewed.to_literal(|id| Err(TermError::not_a_literal(id)), named)?,
            Literal::new_typed_literal(form, NamedNode::new(DATATYPE)?)
        );
    }
    Ok(())
}
