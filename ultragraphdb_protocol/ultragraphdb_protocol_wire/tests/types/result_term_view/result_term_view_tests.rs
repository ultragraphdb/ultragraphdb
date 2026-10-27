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
}
