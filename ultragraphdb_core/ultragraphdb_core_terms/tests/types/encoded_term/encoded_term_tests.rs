/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError, TermErrorEnum, TermView};

fn encode(literal: &Literal) -> EncodedTerm {
    EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(9))
        .unwrap()
        .into_term()
}

/// One encoding of every tag kind.
fn every_kind() -> Vec<EncodedTerm> {
    let other = NamedNode::new_unchecked("http://example.org/other");
    let long = "z".repeat(20);
    let mut terms = vec![EncodedTerm::uid(42)];
    terms.extend(
        [
            Literal::new_typed_literal("true", xsd::BOOLEAN),
            Literal::new_typed_literal("42", xsd::INTEGER),
            Literal::new_typed_literal("1.5", xsd::DECIMAL),
            Literal::new_typed_literal("2026-10-26T12:00:00Z", xsd::DATE_TIME),
            Literal::new_typed_literal("P1Y2M3DT4H5M6S", xsd::DURATION),
            Literal::new_simple_literal("short"),
            Literal::new_simple_literal(long.clone()),
            Literal::new_language_tagged_literal("chat", "fr").unwrap(),
            Literal::new_language_tagged_literal(long.clone(), "fr").unwrap(),
            Literal::new_language_tagged_literal("chat", "zh-Hant-TW-x-abcdefgh").unwrap(),
            Literal::new_directional_language_tagged_literal("chat", "fr", BaseDirection::Rtl)
                .unwrap(),
            Literal::new_typed_literal("042", xsd::INTEGER),
            Literal::new_typed_literal(long.clone(), xsd::INT),
            Literal::new_typed_literal("x", other.clone()),
            Literal::new_typed_literal(long, other),
            Literal::new_typed_literal("[1, 2]", NamedNode::new_unchecked(FLOAT32_VECTOR)),
        ]
        .iter()
        .map(encode),
    );
    terms
}

#[test]
fn test_a_uid_round_trips() {
    let term = EncodedTerm::uid(0xDEAD_BEEF);
    assert_eq!(term.as_uid(), Some(0xDEAD_BEEF));
    assert!(!term.is_literal());
    assert_eq!(term.view(), Ok(TermView::Uid(0xDEAD_BEEF)));
    assert_eq!(EncodedTerm::from_bytes(term.as_bytes()), Ok(term));
}

#[test]
fn test_read_stops_where_the_tag_says_and_returns_the_rest() {
    for term in every_kind() {
        let mut bytes = term.as_bytes().to_vec();
        bytes.extend_from_slice(b"rest");
        let (read, rest) = EncodedTerm::read(&bytes).unwrap();
        assert_eq!((&read, rest), (&term, &b"rest"[..]));
        assert!(
            EncodedTerm::from_bytes(&bytes).is_err(),
            "trailing bytes are refused"
        );
    }
}

#[test]
fn test_every_proper_prefix_of_every_kind_is_refused_without_a_panic() {
    for term in every_kind() {
        let bytes = term.as_bytes();
        for end in 0..bytes.len() {
            let error = EncodedTerm::read(&bytes[..end]).unwrap_err();
            assert!(
                matches!(error.0, TermErrorEnum::Truncated { .. }),
                "{term:?} cut at {end}: {error}"
            );
        }
    }
}

#[test]
fn test_unknown_tags_codes_and_overlong_parts_are_refused() {
    assert_eq!(
        EncodedTerm::read(&[0x7f]).unwrap_err(),
        TermError::UnknownTag(0x7f)
    );
    assert!(matches!(
        EncodedTerm::read(&[0x20, 16]).unwrap_err().0,
        TermErrorEnum::Malformed { .. }
    ));
    let unknown_code = EncodedTerm::from_bytes(&[0x30, 200, 1, b'x']).unwrap();
    assert_eq!(unknown_code.view(), Err(TermError::UnknownDatatype(200)));
    let not_utf8 = EncodedTerm::from_bytes(&[0x20, 1, 0xff]).unwrap();
    assert!(matches!(
        not_utf8.view().unwrap_err().0,
        TermErrorEnum::Malformed { .. }
    ));
    assert!(matches!(
        EncodedTerm::read(&[0x40, 0, 0]).unwrap_err().0,
        TermErrorEnum::Malformed { .. }
    ));
}

#[test]
fn test_the_fingerprint_is_farm_of_the_encoding_and_order_is_byte_order() {
    let terms = every_kind();
    for term in &terms {
        assert_eq!(term.fingerprint(), fingerprint64(term.as_bytes()));
    }
    let mut sorted = terms.clone();
    sorted.sort();
    let mut bytes: Vec<Vec<u8>> = terms.iter().map(|t| t.as_bytes().to_vec()).collect();
    bytes.sort();
    assert_eq!(
        sorted
            .iter()
            .map(|t| t.as_bytes().to_vec())
            .collect::<Vec<_>>(),
        bytes
    );
    assert!(
        terms
            .iter()
            .all(|t| t.is_vector() || t.as_bytes().len() <= 33)
    );
}

#[test]
fn test_check_takes_every_whole_encoding() {
    for term in every_kind() {
        assert_eq!(
            EncodedTerm::check(term.as_bytes()),
            Ok(()),
            "{:02x?}",
            term.as_bytes()
        );
    }
    for bytes in [
        &[0x10, 0][..],
        &[0x10, 1],
        &[0x20, 0],
        &[0x20, 2, 0xC3, 0xA9],
        &[0x30, 1, 1, b'x'],
        &[0x30, 33, 0],
        &[0x22, 1, b'x', 2, b'e', b'n'],
        &[0x40, 1, 0, 0, 0, 0x80, 0x3F],
    ] {
        assert_eq!(EncodedTerm::check(bytes), Ok(()), "{bytes:02x?}");
    }
}

#[test]
fn test_check_refuses_what_a_peer_may_not_send() {
    // PROTOCOL.md section 10.1: each rule of a whole encoding broken once.
    for bytes in [
        // An unknown tag, and the result-only blank-node tag the store never holds.
        &[0x7F][..],
        &[0x02, 0, 0, 0, 0, 0, 0, 0, 6],
        // A boolean other than 0 or 1 (point 50).
        &[0x10, 2],
        &[0x10, 0xFF],
        // Inline text that is not UTF-8 (point 13): a string, a language string's lexical form and its tag, a known
        // datatype's lexical form, another datatype's lexical form.
        &[0x20, 1, 0xFF],
        &[0x22, 1, 0xFF, 2, b'e', b'n'],
        &[0x22, 1, b'x', 2, b'e', 0xFF],
        &[0x30, 1, 1, 0xFF],
        &[0x32, 0, 0, 0, 0, 0, 0, 0, 9, 1, 0xFF],
        // A known datatype's code outside section 10.4 (point 13).
        &[0x30, 0, 1, b'x'],
        &[0x30, 34, 1, b'x'],
        &[0x31, 200, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        // An inline part longer than 15 bytes, a vector of no dimension.
        &[0x20, 16],
        &[0x40, 0, 0],
        // Truncated, and with a byte past the encoding.
        &[0x11, 0, 0],
        &[0x10, 1, 0],
        &[],
    ] {
        assert!(EncodedTerm::check(bytes).is_err(), "{bytes:02x?}");
    }
}
