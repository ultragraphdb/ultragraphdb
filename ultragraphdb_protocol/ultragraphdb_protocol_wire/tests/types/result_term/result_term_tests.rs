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
    assert!(!term.is_blank_node());
    assert_eq!(term.encoding(), EncodedTerm::uid(5).as_bytes());
}

#[test]
fn test_a_blank_node_takes_tag_two_and_its_skolem_iri() {
    let skolem = "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/6";
    let blank = ResultTerm::blank_node(6, skolem);
    assert!(blank.is_blank_node());
    assert_eq!(blank.text(), Some(skolem));
    assert_eq!(blank.annotation(), None);
    // The store's encoding keeps the IRI's tag; the result's takes 0x02.
    assert_eq!(blank.term(), &EncodedTerm::uid(6));
    assert_eq!(blank.encoding(), vec![0x02, 0, 0, 0, 0, 0, 0, 0, 6]);
    assert_ne!(
        blank,
        ResultTerm::new(EncodedTerm::uid(6)).with_text(skolem)
    );
}
