/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode, NamedNodeRef};
use ultragraphdb_core_terms::{
    Datatype, EncodedTerm, FLOAT32_VECTOR, Hash128, KnownDatatype, TermError, TermErrorEnum,
    TermView, Text,
};

/// The UID the tests give a datatype the encoding does not know.
const OTHER_UID: u64 = 0x0102_0304_0506_0708;

fn other_uid(datatype: NamedNodeRef<'_>) -> Result<u64, TermError> {
    assert_eq!(datatype.as_str(), "http://example.org/other");
    Ok(OTHER_UID)
}

/// `literal` encoded, its dictionary strings collected, and decoded back.
fn round_trip(literal: &Literal) -> (EncodedTerm, Literal) {
    let encoded = EncodedTerm::literal(literal.as_ref(), other_uid).expect("encodes");
    let strings: BTreeMap<Hash128, String> = encoded
        .strings()
        .map(|(h, s)| {
            assert_eq!(
                h,
                Hash128::of(s.as_bytes()),
                "a string is named by its own hash"
            );
            (h, s.to_string())
        })
        .collect();
    let term = encoded.into_term();
    let decoded = term
        .view()
        .expect("views")
        .to_literal(
            |uid| {
                assert_eq!(uid, OTHER_UID);
                Ok::<_, TermError>(NamedNode::new_unchecked("http://example.org/other"))
            },
            |h| Ok(strings[&h].clone()),
        )
        .expect("decodes");
    assert_eq!(&decoded, literal, "decoding returns the exact literal");
    let (read, rest) = EncodedTerm::read(term.as_bytes()).expect("reads back");
    assert_eq!((read, rest), (term.clone(), &[][..]));
    (term, decoded)
}

/// Lexical forms of each datatype encoded by value, canonical and not, as oxsdatatypes formats values.
fn by_value_samples() -> Vec<(NamedNodeRef<'static>, Vec<&'static str>)> {
    vec![
        (xsd::BOOLEAN, vec!["true", "false", "1", "0"]),
        (xsd::INTEGER, vec!["42", "-7", "042", "+5", "0"]),
        (xsd::DECIMAL, vec!["1.5", "1.50", "01.5", "-0.25", "3"]),
        (
            xsd::DOUBLE,
            vec!["1.5E0", "1.5e0", "15E-1", "INF", "NaN", "1", "-0"],
        ),
        (xsd::FLOAT, vec!["1.5E0", "1.5e0", "INF", "1"]),
        (
            xsd::DATE_TIME,
            vec![
                "2026-10-26T12:00:00Z",
                "2026-10-26T12:00:00+00:00",
                "2026-10-26T12:00:00.0Z",
                "2026-10-26T12:00:00.5",
            ],
        ),
        (
            xsd::TIME,
            vec!["12:00:00Z", "12:00:00+00:00", "12:00:00.50"],
        ),
        (
            xsd::DATE,
            vec!["2026-10-26", "2026-10-26Z", "2026-10-26+00:00"],
        ),
        (
            xsd::G_YEAR_MONTH,
            vec!["2026-10", "2026-10Z", "2026-10+00:00"],
        ),
        (xsd::G_YEAR, vec!["2026", "2026Z", "2026+00:00"]),
        (
            xsd::G_MONTH_DAY,
            vec!["--10-26", "--10-26Z", "--10-26+00:00"],
        ),
        (xsd::G_DAY, vec!["---26", "---26Z", "---26+00:00"]),
        (xsd::G_MONTH, vec!["--10", "--10Z", "--10+00:00"]),
        (
            xsd::DURATION,
            vec!["P1Y2M3DT4H5M6S", "P1Y2M3DT4H5M6.0S", "PT0S", "P0D", "P14M"],
        ),
        (xsd::YEAR_MONTH_DURATION, vec!["P1Y2M", "P14M", "P0M"]),
        (xsd::DAY_TIME_DURATION, vec!["P1DT2H", "PT26H", "PT0S"]),
    ]
}

#[test]
fn test_each_by_value_datatype_encodes_canonical_forms_by_value_and_keeps_the_others() {
    for (datatype, samples) in by_value_samples() {
        let (mut by_value, mut kept) = (0, 0);
        for lexical_form in samples {
            let literal = Literal::new_typed_literal(lexical_form, datatype);
            let (term, _) = round_trip(&literal);
            match term.view().unwrap() {
                TermView::Typed {
                    datatype: Datatype::Known(k),
                    lexical,
                } => {
                    assert_eq!(k.iri(), datatype.as_str());
                    match lexical {
                        Text::Inline(l) => assert_eq!(l, lexical_form),
                        Text::Hashed(h) => assert!(
                            lexical_form.len() > 15 && h == Hash128::of(lexical_form.as_bytes())
                        ),
                    }
                    kept += 1;
                }
                TermView::Typed { .. } => panic!("{literal} is named by a UID"),
                _ => by_value += 1,
            }
        }
        assert!(
            by_value > 0 && kept > 0,
            "{datatype}: {by_value} by value, {kept} kept"
        );
    }
}

#[test]
fn test_equal_literals_encode_equally_and_unequal_ones_unequally() {
    let mut literals: Vec<Literal> = by_value_samples()
        .into_iter()
        .flat_map(|(d, samples)| {
            samples
                .into_iter()
                .map(move |l| Literal::new_typed_literal(l, d))
        })
        .collect();
    literals.extend([
        Literal::new_simple_literal("1"),
        Literal::new_simple_literal("true"),
        Literal::new_typed_literal("1", xsd::INT),
        Literal::new_typed_literal("1", NamedNode::new_unchecked("http://example.org/other")),
        Literal::new_language_tagged_literal("1", "en").unwrap(),
        Literal::new_directional_language_tagged_literal("1", "en", BaseDirection::Ltr).unwrap(),
        Literal::new_directional_language_tagged_literal("1", "en", BaseDirection::Rtl).unwrap(),
    ]);
    let encoded: Vec<EncodedTerm> = literals.iter().map(|l| round_trip(l).0).collect();
    for (i, a) in literals.iter().enumerate() {
        for (j, b) in literals.iter().enumerate() {
            assert_eq!(a == b, encoded[i] == encoded[j], "{a} against {b}");
        }
    }
}

#[test]
fn test_strings_are_inline_up_to_fifteen_bytes_and_hashed_beyond() {
    let (small, _) = round_trip(&Literal::new_simple_literal("a".repeat(15)));
    assert_eq!(small.as_bytes().len(), 17);
    assert_eq!(
        small.view().unwrap(),
        TermView::String(Text::Inline(&"a".repeat(15)))
    );
    let big = "a".repeat(16);
    let (hashed, _) = round_trip(&Literal::new_simple_literal(big.clone()));
    assert_eq!(
        hashed.view().unwrap(),
        TermView::String(Text::Hashed(Hash128::of(big.as_bytes())))
    );
    let (explicit, _) = round_trip(&Literal::new_typed_literal("a", xsd::STRING));
    assert_eq!(
        explicit,
        round_trip(&Literal::new_simple_literal("a")).0,
        "xsd:string is a simple literal"
    );
    let (unicode, _) = round_trip(&Literal::new_simple_literal("ünïcödé"));
    assert!(unicode.as_bytes().len() <= 17);
}

#[test]
fn test_language_strings_take_inline_hashed_and_both_hashed_forms_in_each_direction() {
    let directions = [None, Some(BaseDirection::Ltr), Some(BaseDirection::Rtl)];
    let long_tag = "zh-Hant-TW-x-abcdefgh";
    for direction in directions {
        let make = |lexical: &str, language: &str| match direction {
            None => Literal::new_language_tagged_literal(lexical, language).unwrap(),
            Some(d) => {
                Literal::new_directional_language_tagged_literal(lexical, language, d).unwrap()
            }
        };
        let cases = [
            (make("chat", "fr"), true, true),
            (make(&"b".repeat(30), "fr"), false, true),
            (make("chat", long_tag), false, false),
        ];
        for (literal, lexical_inline, language_inline) in cases {
            let (term, _) = round_trip(&literal);
            let TermView::LangString {
                lexical,
                language,
                direction: d,
            } = term.view().unwrap()
            else {
                panic!("{literal} is no language string");
            };
            assert_eq!(
                matches!(lexical, Text::Inline(_)),
                lexical_inline,
                "{literal}"
            );
            assert_eq!(
                matches!(language, Text::Inline(_)),
                language_inline,
                "{literal}"
            );
            assert_eq!(d, direction);
            assert!(term.as_bytes().len() <= 33);
        }
    }
}

#[test]
fn test_other_typed_literals_name_their_datatype_by_uid() {
    let other = NamedNode::new_unchecked("http://example.org/other");
    let (small, _) = round_trip(&Literal::new_typed_literal("x", other.clone()));
    assert_eq!(
        small.view().unwrap(),
        TermView::Typed {
            datatype: Datatype::Uid(OTHER_UID),
            lexical: Text::Inline("x")
        }
    );
    let long = "x".repeat(20);
    let (big, _) = round_trip(&Literal::new_typed_literal(long.clone(), other));
    assert_eq!(
        big.view().unwrap(),
        TermView::Typed {
            datatype: Datatype::Uid(OTHER_UID),
            lexical: Text::Hashed(Hash128::of(long.as_bytes()))
        }
    );
    let (known, _) = round_trip(&Literal::new_typed_literal("7", xsd::INT));
    assert_eq!(
        known.view().unwrap(),
        TermView::Typed {
            datatype: Datatype::Known(KnownDatatype::Int),
            lexical: Text::Inline("7")
        }
    );
}

#[test]
fn test_a_known_datatype_takes_no_uid() {
    for literal in [
        Literal::new_typed_literal("x".repeat(30), xsd::INT),
        Literal::new_typed_literal(
            "{}",
            NamedNode::new_unchecked("http://www.opengis.net/ont/geosparql#geoJSONLiteral"),
        ),
    ] {
        EncodedTerm::literal(literal.as_ref(), |_| -> Result<u64, TermError> {
            panic!("no UID is needed")
        })
        .unwrap();
    }
}

#[test]
fn test_a_vector_is_encoded_by_value_and_a_non_canonical_one_is_refused() {
    let vector = |l: &str| Literal::new_typed_literal(l, NamedNode::new_unchecked(FLOAT32_VECTOR));
    let (term, _) = round_trip(&vector("[0.5, -1, 2.25]"));
    assert!(term.is_vector());
    let TermView::Vector(elements) = term.view().unwrap() else {
        panic!("a vector");
    };
    assert_eq!(elements.iter().collect::<Vec<_>>(), vec![0.5, -1.0, 2.25]);
    for bad in ["[0.50, -1]", "[0.5,-1]", "[]", "0.5", "[NaN]", "[inf]"] {
        let error = EncodedTerm::literal(vector(bad).as_ref(), other_uid).unwrap_err();
        assert!(
            matches!(error.0, TermErrorEnum::InvalidVector { .. }),
            "{bad}: {error}"
        );
    }
}

/// The golden vectors of `testdata/by_value/by_value.tsv`, which `generate_by_value.py` computed from PROTOCOL.md
/// section 10.5 alone: each lexical form and the encoding it takes, by value or kept by its lexical form.
fn by_value_vectors() -> Vec<(String, String, Vec<u8>)> {
    let path = ultragraphdb_tool_bdd::testdata_dir()
        .join("by_value")
        .join("by_value.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.lines()
        .map(|line| {
            let mut fields = line.split('\t');
            let (datatype, lexical, hex) = (
                fields.next().unwrap(),
                fields.next().unwrap(),
                fields.next().unwrap(),
            );
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            (datatype.to_string(), lexical.to_string(), bytes)
        })
        .collect()
}

#[test]
fn test_the_by_value_forms_are_the_protocols_own() {
    // Point 12: the canonical forms are normative, pinned against a formatting library's upgrade. Points 11 and 49:
    // an offset of -00:01, and a value past its payload, are kept by their lexical form.
    let vectors = by_value_vectors();
    assert_eq!(vectors.len(), 114);
    let mut failures = Vec::new();
    for (datatype, lexical, want) in &vectors {
        let literal = Literal::new_typed_literal(lexical, NamedNode::new_unchecked(datatype));
        let encoded = EncodedTerm::literal(literal.as_ref(), other_uid).expect("encodes");
        if encoded.term().as_bytes() != want.as_slice() {
            failures.push(format!(
                "{lexical}^^{datatype}: {:02x?}, want {want:02x?}",
                encoded.term().as_bytes()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn test_a_timezone_of_minus_one_minute_is_kept_by_its_lexical_form() {
    for (datatype, lexical) in [
        (xsd::DATE_TIME, "2026-10-26T12:00:00-00:01"),
        (xsd::TIME, "12:00:00-00:01"),
        (xsd::DATE, "2026-10-26-00:01"),
        (xsd::G_YEAR_MONTH, "2026-10-00:01"),
        (xsd::G_YEAR, "2026-00:01"),
        (xsd::G_MONTH_DAY, "--10-26-00:01"),
        (xsd::G_DAY, "---26-00:01"),
        (xsd::G_MONTH, "--10-00:01"),
    ] {
        let literal = Literal::new_typed_literal(lexical, datatype);
        let (term, decoded) = round_trip(&literal);
        assert!(
            matches!(term.as_bytes()[0], 0x30 | 0x31),
            "{lexical} is kept by its lexical form"
        );
        assert_eq!(decoded, literal);
        // One minute east is held by value, and reads back with its offset.
        let east = lexical.replace("-00:01", "+00:01");
        let (term, _) = round_trip(&Literal::new_typed_literal(east.as_str(), datatype));
        assert!(
            (0x15..=0x1C).contains(&term.as_bytes()[0]),
            "{east} is by value"
        );
    }
}

#[test]
fn test_february_29_of_a_gmonthday_is_held_by_value_and_reads_back() {
    // XML Schema 1.1's day-of-month constraint takes --02-29; the payload is February 28's and one day.
    for lexical in ["--02-29", "--02-29Z", "--02-29+05:30"] {
        let literal = Literal::new_typed_literal(lexical, xsd::G_MONTH_DAY);
        let (term, decoded) = round_trip(&literal);
        assert_eq!(term.as_bytes()[0], 0x1A, "{lexical} is by value");
        assert_eq!(decoded, literal);
    }
    let day_before = EncodedTerm::literal(
        Literal::new_typed_literal("--02-28Z", xsd::G_MONTH_DAY).as_ref(),
        other_uid,
    )
    .unwrap()
    .into_term();
    let leap_day = EncodedTerm::literal(
        Literal::new_typed_literal("--02-29Z", xsd::G_MONTH_DAY).as_ref(),
        other_uid,
    )
    .unwrap()
    .into_term();
    let instant = |t: &EncodedTerm| i128::from_be_bytes(t.as_bytes()[1..17].try_into().unwrap());
    assert_eq!(
        instant(&leap_day) - instant(&day_before),
        86_400 * 10i128.pow(18)
    );
    assert!(day_before < leap_day, "terms order as their values");
}
