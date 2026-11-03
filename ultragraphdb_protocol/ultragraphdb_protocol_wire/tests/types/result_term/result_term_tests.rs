/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_protocol_wire::ResultTerm;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::result_batch::result_batch_tests::SKOLEM_6;

#[test]
fn test_an_iri_is_named_by_the_hash_of_its_text() -> TestResult {
    let term = ResultTerm::iri("http://ex.org/a").with_annotation("en");
    // PROTOCOL.md §10.1: tag 0x01, then the first 16 bytes of the SHA-256 digest of the IRI's text.
    let hash = Hash128::of(b"http://ex.org/a");
    assert_eq!(term.hash(), Some(hash));
    assert_eq!(term.term(), None);
    assert_eq!(term.text(), Some("http://ex.org/a"));
    assert_eq!(term.annotation(), Some("en"));
    assert!(!term.is_blank_node());
    let encoding = term.encoding();
    assert_eq!(encoding.len(), 17);
    assert_eq!(encoding.first(), Some(&0x01));
    assert_eq!(encoding.get(1..), Some(&hash.as_bytes()[..]));
    assert_ne!(ResultTerm::iri("http://ex.org/b").encoding(), encoding);
    Ok(())
}

#[test]
fn test_a_literal_keeps_its_encoding_text_and_annotation() -> TestResult {
    let encoded = EncodedTerm::from_bytes(&[0x20, 2, b'A', b'l']).context("an inline string")?;
    let term = ResultTerm::new(encoded.clone())
        .with_text("Al")
        .with_annotation("en");
    assert_eq!(term.term(), Some(&encoded));
    assert_eq!(term.hash(), None);
    assert_eq!(term.text(), Some("Al"));
    assert_eq!(term.annotation(), Some("en"));
    let bare = ResultTerm::new(encoded);
    assert_eq!(bare.text(), None);
    assert_eq!(bare.annotation(), None);
    assert_ne!(bare, term);
    assert!(!term.is_blank_node());
    assert_eq!(term.encoding(), vec![0x20, 2, b'A', b'l']);
    Ok(())
}

#[test]
fn test_a_blank_node_takes_tag_two_and_its_skolem_iris_hash() {
    let blank = ResultTerm::blank_node(SKOLEM_6);
    assert!(blank.is_blank_node());
    assert_eq!(blank.text(), Some(SKOLEM_6));
    assert_eq!(blank.annotation(), None);
    assert_eq!(blank.term(), None);
    let hash = Hash128::of(SKOLEM_6.as_bytes());
    assert_eq!(blank.hash(), Some(hash));
    let mut expected = vec![0x02];
    expected.extend_from_slice(hash.as_bytes());
    assert_eq!(blank.encoding(), expected);
    // An IRI of the same text is another term: its tag is 0x01.
    assert_ne!(blank, ResultTerm::iri(SKOLEM_6));
    assert_ne!(blank.encoding(), ResultTerm::iri(SKOLEM_6).encoding());
}
