/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError, TermView};

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
    assert!(!term.is_vector());
    assert_eq!(term.view(), Ok(TermView::Uid(0xDEAD_BEEF)));
    assert_eq!(EncodedTerm::from_bytes(term.as_bytes()), Ok(term));
}

#[test]
fn test_a_uid_is_its_tag_and_its_value_big_endian() {
    // PROTOCOL.md section 10.1: tag 0x01, then the UID as a u64 big-endian, 9 bytes.
    for (uid, bytes) in [
        (0xDEAD_BEEF, [0x01, 0, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF]),
        (0, [0x01, 0, 0, 0, 0, 0, 0, 0, 0]),
        (
            u64::MAX,
            [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        ),
    ] {
        let term = EncodedTerm::uid(uid);
        assert_eq!(term.as_bytes(), &bytes, "{uid:#x}");
        assert_eq!(term.as_uid(), Some(uid));
    }
    // Big-endian is what orders UIDs numerically by their bytes: 2 < 256, though 256's low byte is the smaller.
    assert!(EncodedTerm::uid(2) < EncodedTerm::uid(256));
}

#[test]
fn test_a_literal_names_no_uid() {
    for term in every_kind().into_iter().skip(1) {
        assert!(term.is_literal(), "{term:?}");
        assert_eq!(term.as_uid(), None, "{term:?}");
        assert_eq!(term.is_vector(), term.as_bytes()[0] == 0x40, "{term:?}");
    }
}

#[test]
fn test_debug_shows_the_encoding_in_hex() {
    assert_eq!(
        format!("{:?}", EncodedTerm::uid(0x0102)),
        "EncodedTerm(010000000000000102)"
    );
}

#[test]
fn test_read_stops_where_the_tag_says_and_returns_the_rest() {
    for term in every_kind() {
        let mut bytes = term.as_bytes().to_vec();
        bytes.extend_from_slice(b"rest");
        let (read, rest) = EncodedTerm::read(&bytes).unwrap();
        assert_eq!((&read, rest), (&term, &b"rest"[..]));
        assert_eq!(
            EncodedTerm::from_bytes(&bytes),
            Err(TermError::Malformed(
                bytes.clone(),
                "bytes follow the encoding"
            )),
            "trailing bytes are refused"
        );
    }
}

#[test]
fn test_every_proper_prefix_of_every_kind_is_refused_without_a_panic() {
    for term in every_kind() {
        let bytes = term.as_bytes();
        for end in 0..bytes.len() {
            assert_eq!(
                EncodedTerm::read(&bytes[..end]).map(|_| ()),
                Err(TermError::Truncated(bytes[..end].to_vec())),
                "{term:?} cut at {end}"
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
    assert_eq!(
        EncodedTerm::read(&[0x20, 16]).map(|_| ()),
        Err(TermError::Malformed(
            vec![0x20, 16],
            "an inline part longer than 15 bytes"
        ))
    );
    let unknown_code = EncodedTerm::from_bytes(&[0x30, 200, 1, b'x']).unwrap();
    assert_eq!(unknown_code.view(), Err(TermError::UnknownDatatype(200)));
    let not_utf8 = EncodedTerm::from_bytes(&[0x20, 1, 0xff]).unwrap();
    assert_eq!(
        not_utf8.view(),
        Err(TermError::Malformed(
            vec![0x20, 1, 0xff],
            "an inline part is not UTF-8"
        ))
    );
    assert_eq!(
        EncodedTerm::read(&[0x40, 0, 0]).map(|_| ()),
        Err(TermError::Malformed(vec![0x40, 0, 0], "an empty vector"))
    );
}

#[test]
fn test_every_inline_length_is_read_where_its_tag_puts_it() {
    // PROTOCOL.md section 10.1: each inline part of each tag, its length byte set to 16 with 16 bytes after it, so
    // the length rule refuses it and not the bytes' end.
    let sixteen = [b'a'; 16];
    let lexical_16 = [&[0x22, 16][..], &sixteen, &[2, b'e', b'n']].concat();
    let language_16 = [&[0x22, 1, b'x', 16][..], &sixteen].concat();
    let big_lang_16 = [&[0x23][..], &[0; 16], &[16], &sixteen].concat();
    let known_16 = [&[0x30, 2, 16][..], &sixteen].concat();
    let typed_16 = [&[0x32][..], &[0; 8], &[16], &sixteen].concat();
    for bytes in [lexical_16, language_16, big_lang_16, known_16, typed_16] {
        assert_eq!(
            EncodedTerm::read(&bytes).map(|_| ()),
            Err(TermError::Malformed(
                bytes.clone(),
                "an inline part longer than 15 bytes"
            )),
            "{bytes:02x?}"
        );
    }
    // Fifteen, the longest inline part, at each place: the encodings end exactly after their parts.
    let fifteen = [b'a'; 15];
    for bytes in [
        [&[0x20, 15][..], &fifteen].concat(),
        [&[0x22, 15][..], &fifteen, &[15], &fifteen].concat(),
        [&[0x23][..], &[0; 16], &[15], &fifteen].concat(),
        [&[0x30, 2, 15][..], &fifteen].concat(),
        [&[0x32][..], &[0; 8], &[15], &fifteen].concat(),
    ] {
        assert_eq!(
            EncodedTerm::read(&bytes).map(|(t, rest)| (t.as_bytes().len(), rest.len())),
            Ok((bytes.len(), 0)),
            "{bytes:02x?}"
        );
    }
}

#[test]
fn test_a_vectors_dimension_is_little_endian() {
    // Dimension 258 is 0x0102, the bytes 02 01; read big-endian it would be 513 and the bytes too few.
    let bytes = [&[0x40, 0x02, 0x01][..], &[0u8; 4 * 258]].concat();
    let (term, rest) = EncodedTerm::read(&bytes).unwrap();
    assert_eq!((term.as_bytes().len(), rest.len()), (3 + 4 * 258, 0));
    let TermView::Vector(view) = term.view().unwrap() else {
        panic!("a vector");
    };
    assert_eq!(view.len(), 258);
    // The widest vector, 65,535 elements: 0xFFFF.
    let widest = [&[0x40, 0xFF, 0xFF][..], &vec![0u8; 4 * 65_535]].concat();
    assert_eq!(EncodedTerm::check(&widest), Ok(()));
}

#[test]
fn test_a_view_refuses_inline_text_that_is_not_utf8_wherever_it_is() {
    for bytes in [
        // A language string's lexical form, then its tag.
        vec![0x22, 1, 0xFF, 2, b'e', b'n'],
        vec![0x25, 1, b'x', 2, b'e', 0xFF],
        // A hashed lexical form's inline tag, in the right-to-left set.
        [&[0x29][..], &[0; 16], &[1, 0xFF]].concat(),
        // A known datatype's and another datatype's lexical form.
        vec![0x30, 2, 1, 0xFF],
        [&[0x32][..], &[0; 8], &[1, 0xFF]].concat(),
    ] {
        let term = EncodedTerm::from_bytes(&bytes).unwrap();
        assert_eq!(
            term.view(),
            Err(TermError::Malformed(
                bytes.clone(),
                "an inline part is not UTF-8"
            )),
            "{bytes:02x?}"
        );
    }
    let hashed_unknown = [&[0x31, 0][..], &[0; 16]].concat();
    assert_eq!(
        EncodedTerm::from_bytes(&hashed_unknown).unwrap().view(),
        Err(TermError::UnknownDatatype(0))
    );
}

#[test]
fn test_the_fingerprint_is_xxh3_of_the_encoding_and_order_is_byte_order() {
    // XXH3-64 from Python's xxhash 3.6.0 (libxxhash 0.8.2) over each encoding's bytes, independently of the Rust code.
    for (term, fingerprint) in [
        (EncodedTerm::uid(42), 5_096_793_477_454_505_956u64),
        (
            encode(&Literal::new_simple_literal("short")),
            4_658_978_779_619_232_382,
        ),
        (
            encode(&Literal::new_typed_literal("42", xsd::INTEGER)),
            13_403_847_476_459_031_323,
        ),
        (
            encode(&Literal::new_typed_literal(
                "[1, 2]",
                NamedNode::new_unchecked(FLOAT32_VECTOR),
            )),
            15_859_647_792_802_437_645,
        ),
    ] {
        assert_eq!(term.fingerprint(), fingerprint, "{term:?}");
    }
    let terms = every_kind();
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
    // PROTOCOL.md section 10.1: each rule of a whole encoding broken once, refused with the rule it breaks.
    let malformed = |bytes: &[u8], reason| Err(TermError::Malformed(bytes.to_vec(), reason));
    let not_utf8 = "inline text that is not UTF-8";
    let cases: Vec<(Vec<u8>, Result<(), TermError>)> = vec![
        // An unknown tag, and the result-only blank-node tag the store never holds.
        (vec![0x7F], Err(TermError::UnknownTag(0x7F))),
        (
            vec![0x02, 0, 0, 0, 0, 0, 0, 0, 6],
            Err(TermError::UnknownTag(0x02)),
        ),
        // A boolean other than 0 or 1 (point 50).
        (
            vec![0x10, 2],
            malformed(&[0x10, 2], "a boolean other than 0 or 1"),
        ),
        (
            vec![0x10, 0xFF],
            malformed(&[0x10, 0xFF], "a boolean other than 0 or 1"),
        ),
        // Inline text that is not UTF-8 (point 13): a string, a language string's lexical form and its tag (in the
        // first and the right-to-left set), a known datatype's lexical form, another datatype's lexical form.
        (vec![0x20, 1, 0xFF], malformed(&[0x20, 1, 0xFF], not_utf8)),
        (
            vec![0x22, 1, 0xFF, 2, b'e', b'n'],
            malformed(&[0x22, 1, 0xFF, 2, b'e', b'n'], not_utf8),
        ),
        (
            vec![0x22, 1, b'x', 2, b'e', 0xFF],
            malformed(&[0x22, 1, b'x', 2, b'e', 0xFF], not_utf8),
        ),
        (
            vec![0x28, 1, b'x', 2, b'e', 0xFF],
            malformed(&[0x28, 1, b'x', 2, b'e', 0xFF], not_utf8),
        ),
        (
            vec![0x30, 1, 1, 0xFF],
            malformed(&[0x30, 1, 1, 0xFF], not_utf8),
        ),
        (
            vec![0x32, 0, 0, 0, 0, 0, 0, 0, 9, 1, 0xFF],
            malformed(&[0x32, 0, 0, 0, 0, 0, 0, 0, 9, 1, 0xFF], not_utf8),
        ),
        // A known datatype's code outside section 10.4 (point 13): 0 and 34 are one below and one above codes 1 to
        // 33, 200 far outside, under the hashed tag.
        (vec![0x30, 0, 1, b'x'], Err(TermError::UnknownDatatype(0))),
        (vec![0x30, 34, 1, b'x'], Err(TermError::UnknownDatatype(34))),
        (
            [&[0x31, 200][..], &[0; 16]].concat(),
            Err(TermError::UnknownDatatype(200)),
        ),
        // An inline part longer than 15 bytes, a vector of no dimension.
        (
            vec![0x20, 16],
            malformed(&[0x20, 16], "an inline part longer than 15 bytes"),
        ),
        (
            vec![0x40, 0, 0],
            malformed(&[0x40, 0, 0], "an empty vector"),
        ),
        // Truncated, and with a byte past the encoding.
        (
            vec![0x11, 0, 0],
            Err(TermError::Truncated(vec![0x11, 0, 0])),
        ),
        (
            vec![0x10, 1, 0],
            malformed(&[0x10, 1, 0], "bytes follow the encoding"),
        ),
        (vec![], Err(TermError::Truncated(vec![]))),
    ];
    for (bytes, expected) in cases {
        assert_eq!(EncodedTerm::check(&bytes), expected, "{bytes:02x?}");
    }
}

#[test]
fn test_check_refuses_a_hashed_lexical_forms_tag_that_is_not_utf8_in_each_direction() {
    // Tags 0x23, 0x26 and 0x29: the lexical form's hash, then the tag inline at byte 17 (PROTOCOL.md section 10.1).
    for tag in [0x23, 0x26, 0x29] {
        let bad = [&[tag][..], &[0; 16], &[2, b'e', 0xFF]].concat();
        assert_eq!(
            EncodedTerm::check(&bad),
            Err(TermError::Malformed(
                bad.clone(),
                "inline text that is not UTF-8"
            )),
            "{tag:#04x}"
        );
        let good = [&[tag][..], &[0xFF; 16], &[2, b'e', b'n']].concat();
        assert_eq!(EncodedTerm::check(&good), Ok(()), "{tag:#04x}");
    }
}

#[test]
fn test_check_takes_the_longest_inline_text_and_the_boundary_values() {
    let fifteen = [&[0x20, 15][..], "ééééééé".as_bytes(), b"a"].concat();
    assert_eq!(EncodedTerm::check(&fifteen), Ok(()));
    // A multi-byte character cut by the length is not UTF-8: 0xC3 alone.
    let cut = [&[0x20, 1][..], &"é".as_bytes()[..1]].concat();
    assert_eq!(
        EncodedTerm::check(&cut),
        Err(TermError::Malformed(
            cut.clone(),
            "inline text that is not UTF-8"
        ))
    );
    let sixteen = [&[0x20, 16][..], &[b'a'; 16]].concat();
    assert_eq!(
        EncodedTerm::check(&sixteen),
        Err(TermError::Malformed(
            sixteen.clone(),
            "an inline part longer than 15 bytes"
        ))
    );
}

#[test]
fn test_check_refuses_a_duration_whose_parts_differ_in_sign() {
    // PROTOCOL.md section 10.5: tag 0x1D holds i64 months, then i128 seconds × 10¹⁸, big-endian; a duration is negative
    // as a whole, so months and seconds of opposite signs are no whole encoding. One month and one second either way.
    let duration = |months: i64, seconds: i128| {
        [&[0x1D][..], &months.to_be_bytes(), &seconds.to_be_bytes()].concat()
    };
    let second = 1_000_000_000_000_000_000i128;
    for bytes in [duration(1, -second), duration(-1, second)] {
        assert_eq!(
            EncodedTerm::check(&bytes),
            Err(TermError::Malformed(
                bytes.clone(),
                "a duration whose months and seconds differ in sign"
            )),
            "{bytes:02x?}"
        );
    }
    // One sign, or a zero part beside either sign: each the encoding of a lexical form.
    for bytes in [
        duration(1, second),
        duration(-1, -second),
        duration(0, -second),
        duration(0, second),
        duration(-1, 0),
        duration(1, 0),
        duration(0, 0),
    ] {
        assert_eq!(EncodedTerm::check(&bytes), Ok(()), "{bytes:02x?}");
    }
    // The encoder's own encoding of a negative duration, `-P1MT1S`, passes.
    let negative = encode(&Literal::new_typed_literal("-P1MT1S", xsd::DURATION));
    assert_eq!(negative.as_bytes(), duration(-1, -second));
    assert_eq!(EncodedTerm::check(negative.as_bytes()), Ok(()));
}

/// Each committed case of the fuzz target `encoded_term`, through the target's steps: the whole-encoding check, and
/// for an encoding it accepts the reader and the literal, which return a value or an error and never panic.
#[test]
fn test_the_encoded_term_fuzz_regressions_replay_without_a_panic() {
    let cases = ultragraphdb_tool_bdd::fuzz_regressions("encoded_term");
    assert!(!cases.is_empty(), "the committed cases");
    for (name, data) in &cases {
        if EncodedTerm::check(data).is_ok() {
            let term = EncodedTerm::from_bytes(data).expect(name);
            let view = term.view().expect(name);
            let _ = view.to_literal(
                |uid| Err::<_, TermError>(TermError::NotALiteral(uid)),
                |_| Ok("the text the hash names".to_string()),
            );
        }
    }
    // The first finding, a duration of one positive and one negative part, whose literal the reader cannot write: its
    // bytes as committed, months 0x00F9FFFFFFFEFFFF and seconds 0xFFFF0000000162626262626262626262.
    let (_, found) = cases
        .iter()
        .find(|(name, _)| name == "duration_parts_of_opposite_signs")
        .expect("the duration case");
    assert_eq!(
        EncodedTerm::check(found),
        Err(TermError::Malformed(
            found.clone(),
            "a duration whose months and seconds differ in sign"
        ))
    );
    // The second, a gDay whose timezone offset is i16::MIN minutes, whose literal the reader cannot write: its bytes
    // as committed, the offset 0x8000 in bytes 17 and 18.
    let (_, found) = cases
        .iter()
        .find(|(name, _)| name == "timezone_offset_of_i16_min")
        .expect("the timezone case");
    assert_eq!(found.len(), 19);
    assert_eq!(&found[17..], &[0x80, 0x00]);
    assert_eq!(
        EncodedTerm::check(found),
        Err(TermError::Malformed(
            found.clone(),
            "a timezone offset beyond 14 hours"
        ))
    );
}

#[test]
fn test_check_refuses_a_timezone_offset_beyond_fourteen_hours() {
    // PROTOCOL.md section 10.5: tags 0x15 to 0x1C hold an i128 `T`, then an i16 `Z`, the offset in minutes or 0xFFFF
    // for none; XML Schema's offsets run from -14:00 to +14:00, -840 to 840 minutes.
    let temporal = |tag: u8, minutes: i16| [&[tag][..], &[0; 16], &minutes.to_be_bytes()].concat();
    for tag in 0x15..=0x1C {
        for minutes in [841, -841, i16::MIN, i16::MAX] {
            let bytes = temporal(tag, minutes);
            assert_eq!(
                EncodedTerm::check(&bytes),
                Err(TermError::Malformed(
                    bytes.clone(),
                    "a timezone offset beyond 14 hours"
                )),
                "{bytes:02x?}"
            );
        }
        // The bounds, UTC, and 0xFFFF, which reads as -1 and marks no timezone.
        for minutes in [840, -840, 0, -1] {
            let bytes = temporal(tag, minutes);
            assert_eq!(EncodedTerm::check(&bytes), Ok(()), "{bytes:02x?}");
        }
    }
    // The encoder's own encoding of a date at +14:00 passes.
    let east = encode(&Literal::new_typed_literal("2026-11-01+14:00", xsd::DATE));
    assert_eq!(&east.as_bytes()[17..], &840i16.to_be_bytes());
    assert_eq!(EncodedTerm::check(east.as_bytes()), Ok(()));
}
