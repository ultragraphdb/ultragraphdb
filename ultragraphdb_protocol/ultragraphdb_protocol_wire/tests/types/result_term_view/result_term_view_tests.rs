/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128, TermView, Text};
use ultragraphdb_protocol_wire::{BodyView, ResultView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::result_batch::result_batch_tests::SKOLEM_6;
use crate::utils::golden_bodies::{frame_of, golden};

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
