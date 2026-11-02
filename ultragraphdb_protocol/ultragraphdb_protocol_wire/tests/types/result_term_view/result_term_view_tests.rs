/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, TermView, Text};
use ultragraphdb_protocol_wire::{BodyView, ResultView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_a_term_reads_through_core_terms_view() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    let al = view.term(1).context("term 1 of the batch")?;
    assert_eq!(al.encoding(), &[0x20, 2, b'A', b'l']);
    assert_eq!(al.text(), None);
    assert_eq!(al.annotation(), None);
    let term: EncodedTerm = al.term().context("the term in the store's encoding")?;
    assert_eq!(term.view()?, TermView::String(Text::Inline("Al")));
    let a = view.term(0).context("term 0 of the batch")?;
    assert_eq!(
        a.term()
            .context("the term in the store's encoding")?
            .view()?,
        TermView::Uid(5)
    );
    assert_eq!(a.text(), Some("http://ex.org/a"));
    assert!(!a.is_blank_node());
    Ok(())
}

#[test]
fn test_a_blank_node_reads_as_the_stores_uid_with_its_own_tag() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    let blank = view.term(3).context("term 3 of the batch")?;
    assert!(blank.is_blank_node());
    assert_eq!(blank.encoding(), &[0x02, 0, 0, 0, 0, 0, 0, 0, 6]);
    assert_eq!(blank.term(), Some(EncodedTerm::uid(6)));
    assert_eq!(
        blank
            .term()
            .context("the term in the store's encoding")?
            .view()?,
        TermView::Uid(6)
    );
    Ok(())
}
