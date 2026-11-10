/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError, TermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

fn encode(literal: &Literal) -> TestResult<EncodedTerm> {
    let datatype = id(1, 9, 9)?;
    Ok(EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(datatype))?.into_term())
}

/// One encoding of every tag kind.
fn every_kind() -> TestResult<Vec<EncodedTerm>> {
    let other = NamedNode::new_unchecked("http://example.org/other");
    let long = "z".repeat(20);
    let mut terms = vec![EncodedTerm::id(id(1, 42, 42)?)];
    terms.extend(
        [
            Literal::new_typed_literal("true", xsd::BOOLEAN),
            Literal::new_typed_literal("42", xsd::INTEGER),
            Literal::new_typed_literal("1.5", xsd::DECIMAL),
            Literal::new_typed_literal("2026-10-26T12:00:00Z", xsd::DATE_TIME),
            Literal::new_typed_literal("P1Y2M3DT4H5M6S", xsd::DURATION),
            Literal::new_simple_literal("short"),
            Literal::new_simple_literal(long.clone()),
            Literal::new_language_tagged_literal("chat", "fr")?,
            Literal::new_language_tagged_literal(long.clone(), "fr")?,
            Literal::new_language_tagged_literal("chat", "zh-Hant-TW-x-abcdefgh")?,
            Literal::new_directional_language_tagged_literal("chat", "fr", BaseDirection::Rtl)?,
            Literal::new_typed_literal("042", xsd::INTEGER),
            Literal::new_typed_literal(long.clone(), xsd::INT),
            Literal::new_typed_literal("x", other.clone()),
            Literal::new_typed_literal(long, other),
            Literal::new_typed_literal("[1, 2]", NamedNode::new_unchecked(FLOAT32_VECTOR)),
        ]
        .iter()
        .map(encode)
        .collect::<TestResult<Vec<_>>>()?,
    );
    Ok(terms)
}

#[test]
fn test_an_id_round_trips() -> TestResult {
    let term = EncodedTerm::id(id(1, 0xAB_CDEF, 0xDEAD_BEEF)?);
    assert_eq!(term.as_id(), Some(id(1, 0xAB_CDEF, 0xDEAD_BEEF)?));
    assert!(!term.is_literal());
    assert!(!term.is_vector());
    assert_eq!(
        term.view(),
        Ok(TermView::Id(id(1, 0xAB_CDEF, 0xDEAD_BEEF)?))
    );
    assert_eq!(EncodedTerm::from_bytes(term.as_bytes()), Ok(term));
    Ok(())
}

#[test]
fn test_an_id_is_its_tag_and_its_16_bytes_big_endian() -> TestResult {
    // Tag 0x01, then the ID's 16 bytes, written by hand: type prefix (2), sequence (8), bucket (3), 3 zero bytes.
    for (term, bytes) in [
        (
            id(1, 42, 42)?,
            [0x01, 0, 1, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 42, 0, 0, 0],
        ),
        (
            id(u16::MAX, 0xFF_FFFF, u64::MAX)?,
            [
                0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0, 0, 0,
            ],
        ),
    ] {
        let encoded = EncodedTerm::id(term);
        assert_eq!(encoded.as_bytes(), &bytes, "{term:?}");
        assert_eq!(encoded.as_id(), Some(term));
    }
    // Big-endian orders IDs as their numbers: by type, then sequence, then bucket. Sequence 2 sorts before 256 though
    // 256's low byte is the smaller, and a larger bucket under a smaller sequence sorts first.
    assert!(EncodedTerm::id(id(1, 0, 2)?) < EncodedTerm::id(id(1, 0, 256)?));
    assert!(EncodedTerm::id(id(1, 0xFF_FFFF, 1)?) < EncodedTerm::id(id(1, 0, 2)?));
    assert!(EncodedTerm::id(id(1, 0, u64::MAX)?) < EncodedTerm::id(id(2, 0, 1)?));
    Ok(())
}

#[test]
fn test_an_id_encoding_of_no_valid_id_names_no_id() -> TestResult {
    // An encoding's 17 bytes whose ID has sequence 0: the length rules take it, the ID does not decode.
    let bytes = [0x01, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0];
    let term = EncodedTerm::from_bytes(&bytes)?;
    assert_eq!(term.as_id(), None);
    // Its tag names an IRI or a blank node, whether or not its ID decodes.
    assert!(!term.is_literal());
    assert_eq!(
        term.view(),
        Err(TermError::malformed(
            bytes.to_vec(),
            "an ID that is no valid system ID"
        ))
    );
    // Eight bytes after the tag, a stage-5 UID's length, are a cut ID.
    let short = [0x01, 0, 0, 0, 0, 0, 0, 0, 42];
    assert_eq!(
        EncodedTerm::from_bytes(&short),
        Err(TermError::truncated(short.to_vec()))
    );
    Ok(())
}

#[test]
fn test_a_literal_names_no_id() -> TestResult {
    for term in every_kind()?.into_iter().skip(1) {
        assert!(term.is_literal(), "{term:?}");
        assert_eq!(term.as_id(), None, "{term:?}");
        assert_eq!(term.is_vector(), term.as_bytes()[0] == 0x40, "{term:?}");
    }
    Ok(())
}

#[test]
fn test_a_literal_whose_16_bytes_are_a_valid_id_names_no_id() -> TestResult {
    // PROTOCOL.md section 10.5: tag 0x12, then the decimal × 10¹⁸ as a big-endian i128. -64 × 10¹⁸ is -(2²⁴ × 5¹⁸),
    // written by hand from Python's `(-64 * 10**18) & (2**128 - 1)`: type prefix 0xFFFF, a non-zero sequence, bucket
    // 0x531627, reserved bits zero, so the 16 bytes after the tag decode as a system ID. The tag alone says literal.
    let term = encode(&Literal::new_typed_literal("-64", xsd::DECIMAL))?;
    let bytes = [
        0x12, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFC, 0x87, 0xD2, 0x53, 0x16, 0x27, 0x00,
        0x00, 0x00,
    ];
    assert_eq!(term.as_bytes(), &bytes);
    assert!(
        SystemId::decode(&bytes[1..]).is_ok(),
        "the payload is an ID"
    );
    assert_eq!(term.as_id(), None);
    assert!(term.is_literal());
    Ok(())
}

#[test]
fn test_debug_shows_the_encoding_in_hex() -> TestResult {
    assert_eq!(
        format!("{:?}", EncodedTerm::id(id(1, 2, 0x0102)?)),
        "EncodedTerm(0100010000000000000102000002000000)"
    );
    Ok(())
}

#[test]
fn test_read_stops_where_the_tag_says_and_returns_the_rest() -> TestResult {
    for term in every_kind()? {
        let mut bytes = term.as_bytes().to_vec();
        bytes.extend_from_slice(b"rest");
        let (read, rest) = EncodedTerm::read(&bytes)?;
        assert_eq!((&read, rest), (&term, &b"rest"[..]));
        assert_eq!(
            EncodedTerm::from_bytes(&bytes),
            Err(TermError::malformed(
                bytes.clone(),
                "bytes follow the encoding"
            )),
            "trailing bytes are refused"
        );
    }
    Ok(())
}

#[test]
fn test_every_proper_prefix_of_every_kind_is_refused_without_a_panic() -> TestResult {
    for term in every_kind()? {
        let bytes = term.as_bytes();
        for end in 0..bytes.len() {
            assert_eq!(
                EncodedTerm::read(&bytes[..end]).map(|_| ()),
                Err(TermError::truncated(bytes[..end].to_vec())),
                "{term:?} cut at {end}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_unknown_tags_codes_and_overlong_parts_are_refused() -> TestResult {
    assert_eq!(
        EncodedTerm::read(&[0x7f]).err(),
        Some(TermError::unknown_tag(0x7f))
    );
    assert_eq!(
        EncodedTerm::read(&[0x20, 16]).map(|_| ()),
        Err(TermError::malformed(
            vec![0x20, 16],
            "an inline part longer than 15 bytes"
        ))
    );
    let unknown_code = EncodedTerm::from_bytes(&[0x30, 200, 1, b'x'])?;
    assert_eq!(unknown_code.view(), Err(TermError::unknown_datatype(200)));
    let not_utf8 = EncodedTerm::from_bytes(&[0x20, 1, 0xff])?;
    assert_eq!(
        not_utf8.view(),
        Err(TermError::malformed(
            vec![0x20, 1, 0xff],
            "an inline part is not UTF-8"
        ))
    );
    assert_eq!(
        EncodedTerm::read(&[0x40, 0, 0]).map(|_| ()),
        Err(TermError::malformed(vec![0x40, 0, 0], "an empty vector"))
    );
    Ok(())
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
    let typed_16 = [&[0x32][..], &[0; 16], &[16], &sixteen].concat();
    for bytes in [lexical_16, language_16, big_lang_16, known_16, typed_16] {
        assert_eq!(
            EncodedTerm::read(&bytes).map(|_| ()),
            Err(TermError::malformed(
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
        [&[0x32][..], &[0; 16], &[15], &fifteen].concat(),
    ] {
        assert_eq!(
            EncodedTerm::read(&bytes).map(|(t, rest)| (t.as_bytes().len(), rest.len())),
            Ok((bytes.len(), 0)),
            "{bytes:02x?}"
        );
    }
}

#[test]
fn test_a_vectors_dimension_is_little_endian() -> TestResult {
    // Dimension 258 is 0x0102, the bytes 02 01; read big-endian it would be 513 and the bytes too few.
    let bytes = [&[0x40, 0x02, 0x01][..], &[0u8; 4 * 258]].concat();
    let (term, rest) = EncodedTerm::read(&bytes)?;
    assert_eq!((term.as_bytes().len(), rest.len()), (3 + 4 * 258, 0));
    let view = term.view()?;
    let len = if let TermView::Vector(v) = &view {
        Some(v.len())
    } else {
        None
    };
    assert_eq!(len, Some(258), "{view:?}");
    // The widest vector, 65,535 elements: 0xFFFF.
    let widest = [&[0x40, 0xFF, 0xFF][..], &vec![0u8; 4 * 65_535]].concat();
    assert_eq!(EncodedTerm::check(&widest), Ok(()));
    Ok(())
}

#[test]
fn test_a_view_refuses_inline_text_that_is_not_utf8_wherever_it_is() -> TestResult {
    for bytes in [
        // A language string's lexical form, then its tag.
        vec![0x22, 1, 0xFF, 2, b'e', b'n'],
        vec![0x25, 1, b'x', 2, b'e', 0xFF],
        // A hashed lexical form's inline tag, in the right-to-left set.
        [&[0x29][..], &[0; 16], &[1, 0xFF]].concat(),
        // A known datatype's and another datatype's lexical form, the other datatype's ID (type 1, sequence 1, bucket
        // 0) valid.
        vec![0x30, 2, 1, 0xFF],
        [&[0x32, 0, 1][..], &[0; 7], &[1], &[0; 6], &[1, 0xFF]].concat(),
    ] {
        let term = EncodedTerm::from_bytes(&bytes)?;
        assert_eq!(
            term.view(),
            Err(TermError::malformed(
                bytes.clone(),
                "an inline part is not UTF-8"
            )),
            "{bytes:02x?}"
        );
    }
    let hashed_unknown = [&[0x31, 0][..], &[0; 16]].concat();
    assert_eq!(
        EncodedTerm::from_bytes(&hashed_unknown)?.view(),
        Err(TermError::unknown_datatype(0))
    );
    Ok(())
}

#[test]
fn test_the_fingerprint_is_xxh3_of_the_encoding_and_order_is_byte_order() -> TestResult {
    // XXH3-64 from Python's xxhash 3.6.0 (libxxhash 0.8.2) over each encoding's bytes, independently of the Rust code.
    for (term, fingerprint) in [
        (
            EncodedTerm::id(id(1, 42, 42)?),
            6_006_982_149_053_221_746u64,
        ),
        (
            encode(&Literal::new_simple_literal("short"))?,
            4_658_978_779_619_232_382,
        ),
        (
            encode(&Literal::new_typed_literal("42", xsd::INTEGER))?,
            13_403_847_476_459_031_323,
        ),
        (
            encode(&Literal::new_typed_literal(
                "[1, 2]",
                NamedNode::new_unchecked(FLOAT32_VECTOR),
            ))?,
            15_859_647_792_802_437_645,
        ),
    ] {
        assert_eq!(term.fingerprint(), fingerprint, "{term:?}");
    }
    let terms = every_kind()?;
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
    Ok(())
}

#[test]
fn test_check_takes_every_whole_encoding() -> TestResult {
    for term in every_kind()? {
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
    Ok(())
}

#[test]
fn test_check_refuses_what_a_peer_may_not_send() {
    // PROTOCOL.md section 10.1: each rule of a whole encoding broken once, refused with the rule it breaks.
    let malformed = |bytes: &[u8], reason| Err(TermError::malformed(bytes.to_vec(), reason));
    let not_utf8 = "inline text that is not UTF-8";
    let cases: Vec<(Vec<u8>, Result<(), TermError>)> = vec![
        // An unknown tag, and the result-only blank-node tag the store never holds.
        (vec![0x7F], Err(TermError::unknown_tag(0x7F))),
        (
            vec![0x02, 0, 0, 0, 0, 0, 0, 0, 6],
            Err(TermError::unknown_tag(0x02)),
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
            [&[0x32][..], &[0; 15], &[9, 1, 0xFF]].concat(),
            malformed(&[&[0x32][..], &[0; 15], &[9, 1, 0xFF]].concat(), not_utf8),
        ),
        // A known datatype's code outside section 10.4 (point 13): 0 and 37 are one below and one above codes 1 to
        // 36, 200 far outside, under the hashed tag.
        (vec![0x30, 0, 1, b'x'], Err(TermError::unknown_datatype(0))),
        (
            vec![0x30, 37, 1, b'x'],
            Err(TermError::unknown_datatype(37)),
        ),
        (
            [&[0x31, 200][..], &[0; 16]].concat(),
            Err(TermError::unknown_datatype(200)),
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
            Err(TermError::truncated(vec![0x11, 0, 0])),
        ),
        (
            vec![0x10, 1, 0],
            malformed(&[0x10, 1, 0], "bytes follow the encoding"),
        ),
        (vec![], Err(TermError::truncated(vec![]))),
    ];
    for (bytes, expected) in cases {
        assert_eq!(EncodedTerm::check(&bytes), expected, "{bytes:02x?}");
    }
    // Codes 34 to 36, the last three section 10.4 lists, are known.
    for code in 34..=36 {
        assert_eq!(EncodedTerm::check(&[0x30, code, 1, b'x']), Ok(()), "{code}");
    }
}

#[test]
fn test_check_refuses_a_hashed_lexical_forms_tag_that_is_not_utf8_in_each_direction() {
    // Tags 0x23, 0x26 and 0x29: the lexical form's hash, then the tag inline at byte 17 (PROTOCOL.md section 10.1).
    for tag in [0x23, 0x26, 0x29] {
        let bad = [&[tag][..], &[0; 16], &[2, b'e', 0xFF]].concat();
        assert_eq!(
            EncodedTerm::check(&bad),
            Err(TermError::malformed(
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
        Err(TermError::malformed(
            cut.clone(),
            "inline text that is not UTF-8"
        ))
    );
    let sixteen = [&[0x20, 16][..], &[b'a'; 16]].concat();
    assert_eq!(
        EncodedTerm::check(&sixteen),
        Err(TermError::malformed(
            sixteen.clone(),
            "an inline part longer than 15 bytes"
        ))
    );
}

#[test]
fn test_check_refuses_a_duration_whose_parts_differ_in_sign() -> TestResult {
    // PROTOCOL.md section 10.5: tag 0x1D holds i64 months, then i128 seconds × 10¹⁸, big-endian; a duration is negative
    // as a whole, so months and seconds of opposite signs are no whole encoding. One month and one second either way.
    let duration = |months: i64, seconds: i128| {
        [&[0x1D][..], &months.to_be_bytes(), &seconds.to_be_bytes()].concat()
    };
    let second = 1_000_000_000_000_000_000i128;
    for bytes in [duration(1, -second), duration(-1, second)] {
        assert_eq!(
            EncodedTerm::check(&bytes),
            Err(TermError::malformed(
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
    let negative = encode(&Literal::new_typed_literal("-P1MT1S", xsd::DURATION))?;
    assert_eq!(negative.as_bytes(), duration(-1, -second));
    assert_eq!(EncodedTerm::check(negative.as_bytes()), Ok(()));
    Ok(())
}

/// Each committed case of the fuzz target `encoded_term`, through the target's steps: the whole-encoding check, and
/// for an encoding it accepts the reader, which opens it or refuses an ID that is no valid system ID, and the literal,
/// which return a value or an error and never panic.
#[test]
fn test_the_encoded_term_fuzz_regressions_replay_without_a_panic() -> TestResult {
    let cases = ultragraphdb_tool_bdd::fuzz_regressions("encoded_term")?;
    assert!(!cases.is_empty(), "the committed cases");
    let invalid_id =
        |data: &[u8]| TermError::malformed(data.to_vec(), "an ID that is no valid system ID");
    for (name, data) in &cases {
        if EncodedTerm::check(data).is_ok() {
            let term = EncodedTerm::from_bytes(data).context(name)?;
            let view = match term.view() {
                Ok(view) => view,
                Err(refusal) => {
                    assert_eq!(refusal, invalid_id(data), "{name}");
                    continue;
                }
            };
            // A literal or an error, either of which passes: the replay asks only that nothing panics.
            let _literal: Result<Literal, TermError> = view.to_literal(
                |id| Err::<_, TermError>(TermError::not_a_literal(id)),
                |_| Ok("the text the hash names".to_string()),
            );
        }
    }
    // The first finding, a duration of one positive and one negative part, whose literal the reader cannot write: its
    // bytes as committed, months 0x00F9FFFFFFFEFFFF and seconds 0xFFFF0000000162626262626262626262.
    let (_, duration) = cases
        .iter()
        .find(|(name, _)| name == "duration_parts_of_opposite_signs")
        .context("the duration case")?;
    assert_eq!(
        EncodedTerm::check(duration),
        Err(TermError::malformed(
            duration.clone(),
            "a duration whose months and seconds differ in sign"
        ))
    );
    // The second, a gDay whose timezone offset is i16::MIN minutes, whose literal the reader cannot write: its bytes
    // as committed, the offset 0x8000 in bytes 17 and 18.
    let (_, timezone) = cases
        .iter()
        .find(|(name, _)| name == "timezone_offset_of_i16_min")
        .context("the timezone case")?;
    assert_eq!(timezone.len(), 19);
    assert_eq!(&timezone[17..], &[0x80, 0x00]);
    assert_eq!(
        EncodedTerm::check(timezone),
        Err(TermError::malformed(
            timezone.clone(),
            "a timezone offset beyond 14 hours"
        ))
    );
    // The third, an ID's tag and 16 bytes that are no valid system ID: whole by section 10.1, where a Result puts a
    // hash there, and refused by the store's reader, which takes them as an ID.
    let (_, id) = cases
        .iter()
        .find(|(name, _)| name == "id_of_nonzero_reserved_bits")
        .context("the ID case")?;
    assert!(SystemId::decode(id.get(1..).context("the ID's bytes")?).is_err());
    assert_eq!(EncodedTerm::check(id), Ok(()));
    assert_eq!(
        EncodedTerm::from_bytes(id)?.view().err(),
        Some(invalid_id(id))
    );
    Ok(())
}

#[test]
fn test_check_refuses_a_timezone_offset_beyond_fourteen_hours() -> TestResult {
    // PROTOCOL.md section 10.5: tags 0x15 to 0x1C hold an i128 `T`, then an i16 `Z`, the offset in minutes or 0xFFFF
    // for none; XML Schema's offsets run from -14:00 to +14:00, -840 to 840 minutes.
    let temporal = |tag: u8, minutes: i16| [&[tag][..], &[0; 16], &minutes.to_be_bytes()].concat();
    for tag in 0x15..=0x1C {
        for minutes in [841, -841, i16::MIN, i16::MAX] {
            let bytes = temporal(tag, minutes);
            assert_eq!(
                EncodedTerm::check(&bytes),
                Err(TermError::malformed(
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
    let east = encode(&Literal::new_typed_literal("2026-11-01+14:00", xsd::DATE))?;
    assert_eq!(&east.as_bytes()[17..], &840i16.to_be_bytes());
    assert_eq!(EncodedTerm::check(east.as_bytes()), Ok(()));
    Ok(())
}

#[test]
fn test_the_three_tags_of_section_10_4s_codes_34_to_36_take_their_tables_lengths() -> TestResult {
    // PROTOCOL.md section 10.1: 0x41, a bfloat16vector, is its dimension, a little-endian u16, then that many 2-byte
    // elements, 3 + 2 × dimension bytes; 0x50, a measure, an 8-byte number then its unit's code as inline text, 10 + n;
    // 0x51, a precision, one byte, 2. Dimension 258 is the bytes 02 01; the unit "m/s" is 3 bytes.
    let bf16 = [&[0x41, 0x02, 0x01][..], &[0u8; 2 * 258]].concat();
    let measure = [&[0x50][..], &1.5f64.to_be_bytes(), &[3], b"m/s"].concat();
    let precision = vec![0x51, 12];
    for (bytes, len) in [(bf16, 3 + 2 * 258), (measure, 10 + 3), (precision, 2)] {
        let with_rest = [&bytes[..], b"rest"].concat();
        let (term, rest) = EncodedTerm::read(&with_rest)?;
        assert_eq!(
            (term.as_bytes(), rest),
            (&bytes[..], &b"rest"[..]),
            "{:#04x}",
            bytes[0]
        );
        assert_eq!(term.as_bytes().len(), len);
        assert_eq!(EncodedTerm::check(&bytes), Ok(()), "{:#04x}", bytes[0]);
    }
    // A bfloat16vector of dimension 3, where 2 × 3 and 2 + 3 differ, and of no dimension.
    let three = [0x41, 3, 0, 0x80, 0x3F, 0, 0x40, 0x40, 0x40];
    assert_eq!(EncodedTerm::check(&three), Ok(()));
    assert_eq!(
        EncodedTerm::read(&[0x41, 0, 0]).map(|_| ()),
        Err(TermError::malformed(vec![0x41, 0, 0], "an empty vector"))
    );
    Ok(())
}

#[test]
fn test_check_refuses_a_measure_without_a_unit_and_a_precision_outside_1_to_12() {
    // PROTOCOL.md sections 10.1 and 10.5: a measure's unit code is at least 1 byte of UTF-8, and a precision's byte is
    // 1 (millennium) to 12 (nanosecond); any other is no whole encoding.
    let number = 1.5f64.to_be_bytes();
    let measure = |unit: &[u8]| [&[0x50][..], &number, &[unit.len() as u8], unit].concat();
    let malformed = |bytes: &[u8], reason| Err(TermError::malformed(bytes.to_vec(), reason));
    let cases: Vec<(Vec<u8>, Result<(), TermError>)> = vec![
        (measure(b""), malformed(&measure(b""), "a measure without a unit")),
        (
            measure(&[0xFF]),
            malformed(&measure(&[0xFF]), "inline text that is not UTF-8"),
        ),
        (measure(b"m"), Ok(())),
        (measure("µm".as_bytes()), Ok(())),
        (
            vec![0x51, 0],
            malformed(&[0x51, 0], "a precision other than 1 to 12"),
        ),
        (
            vec![0x51, 13],
            malformed(&[0x51, 13], "a precision other than 1 to 12"),
        ),
        (vec![0x51, 1], Ok(())),
        (vec![0x51, 12], Ok(())),
    ];
    for (bytes, expected) in cases {
        assert_eq!(EncodedTerm::check(&bytes), expected, "{bytes:02x?}");
    }
}
