/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::ResultTerm;

#[test]
fn test_a_result_term_keeps_its_encoding_text_and_annotation() {
    let term = ResultTerm::new(EncodedTerm::uid(5))
        .with_text("http://ex.org/a")
        .with_annotation("en");
    assert_eq!(term.term().as_uid(), Some(5));
    assert_eq!(term.text(), Some("http://ex.org/a"));
    assert_eq!(term.annotation(), Some("en"));
    let bare = ResultTerm::new(EncodedTerm::uid(5));
    assert_eq!(bare.text(), None);
    assert_eq!(bare.annotation(), None);
    assert_ne!(bare, term);
}
