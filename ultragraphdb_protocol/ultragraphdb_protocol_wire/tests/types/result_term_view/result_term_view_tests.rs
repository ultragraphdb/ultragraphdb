/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, TermView, Text};
use ultragraphdb_protocol_wire::{BodyView, ResultView};

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_a_term_reads_through_core_terms_view() {
    let bytes = golden("result").bytes();
    let view = ResultView::from_frame(&frame_of(&bytes)).unwrap();
    let al = view.term(1).unwrap();
    assert_eq!(al.encoding(), &[0x20, 2, b'A', b'l']);
    assert_eq!(al.text(), None);
    assert_eq!(al.annotation(), None);
    let term: EncodedTerm = al.term().unwrap();
    assert_eq!(term.view().unwrap(), TermView::String(Text::Inline("Al")));
    let a = view.term(0).unwrap();
    assert_eq!(a.term().unwrap().view().unwrap(), TermView::Uid(5));
    assert_eq!(a.text(), Some("http://ex.org/a"));
    assert!(!a.is_blank_node());
}

#[test]
fn test_a_blank_node_reads_as_the_stores_uid_with_its_own_tag() {
    let bytes = golden("result").bytes();
    let view = ResultView::from_frame(&frame_of(&bytes)).unwrap();
    let blank = view.term(3).unwrap();
    assert!(blank.is_blank_node());
    assert_eq!(blank.encoding(), &[0x02, 0, 0, 0, 0, 0, 0, 0, 6]);
    assert_eq!(blank.term(), Some(EncodedTerm::uid(6)));
    assert_eq!(blank.term().unwrap().view().unwrap(), TermView::Uid(6));
}
