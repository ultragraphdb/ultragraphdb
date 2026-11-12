/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyView, ColumnType, Language, PatternSlot, Prepare, PrepareView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, reframe, section, view_any, with_command_header, with_section,
};

#[test]
fn test_the_golden_prepares_read_their_values() -> TestResult {
    let bytes = golden("prepare_reply")?.bytes()?;
    let view = PrepareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.tenant(), view.statement(), view.schema()),
        (0, 1, 0x5C4E_3A2B_1D0F_7E6A)
    );
    assert_eq!(
        view.pattern().context("a pattern")?.subject(),
        PatternSlot::Parameter(0)
    );
    assert_eq!(view.terms().len(), 1);
    assert_eq!(view.parameter_types(), &[ColumnType::Node]);
    assert_eq!(view.result_types(), &[ColumnType::Term, ColumnType::Node]);
    let (terms, p) = pattern();
    assert_eq!(
        view.to_body()?,
        Prepare::new(
            0,
            1,
            0x5C4E_3A2B_1D0F_7E6A,
            terms,
            p,
            vec![ColumnType::Node],
            vec![ColumnType::Term, ColumnType::Node]
        )
    );
    let request_bytes = golden("prepare")?.bytes()?;
    let request_view = PrepareView::from_frame(&frame_of(&request_bytes)?)?;
    assert!(request_view.result_types().is_empty());
    assert_eq!(request_view.statement(), 0);
    Ok(())
}

#[test]
fn test_types_are_known_and_counted() -> TestResult {
    let bytes = golden("prepare_reply")?.bytes()?;
    let data = body_data(&bytes).to_vec();
    let (params, _) = section(&data, 0, 3);
    let (results, _) = section(&data, 0, 4);
    let mut unknown = data.clone();
    unknown[params] = 3;
    assert_eq!(
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &unknown))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 3
        }
    );
    let mut result = data.clone();
    result[results] = 0;
    assert_eq!(
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &result))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 0
        }
    );
    let none = with_section(&bytes, 10, 3, |types| types.clear());
    assert_eq!(
        PrepareView::from_frame(&frame_of(&none)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "parameter types",
            count: 0,
            expected: 1
        }
    );
    let two = with_section(&bytes, 10, 4, |types| types.push(2));
    assert_eq!(
        PrepareView::from_frame(&frame_of(&two)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 3,
            expected: 2
        }
    );
    Ok(())
}

#[test]
fn test_a_request_carries_no_result_types_and_an_answer_one_per_unbound_slot_and_the_reifier()
-> TestResult {
    let request = golden("prepare")?.bytes()?;
    let reply = golden("prepare_reply")?.bytes()?;
    let request_view = PrepareView::from_frame(&frame_of(&request)?)?;
    let reply_view = PrepareView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(request_view.check_request(), Ok(()));
    assert_eq!(reply_view.check_reply(), Ok(()));
    assert_eq!(
        reply_view.check_request().err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 2,
            expected: 0
        }
    );
    // The request read as an answer: no result type for its one unbound slot and the reifier column.
    assert_eq!(
        request_view.check_reply().err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 0,
            expected: 2
        }
    );
    Ok(())
}

fn text_bytes(name: &str) -> TestResult<Vec<u8>> {
    golden(name)?.bytes()
}

fn refused(frame: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(PrepareView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_the_golden_texts_read_their_values() -> TestResult {
    let bytes = text_bytes("prepare_text")?;
    let view = PrepareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.language(), Language::Gql);
    assert_eq!(view.pattern(), None);
    assert!(view.text().contains("$who"));
    assert_eq!(view.names().iter().collect::<Vec<_>>(), ["who"]);
    assert_eq!(view.parameter_types(), [ColumnType::Node]);
    view.check_request()?;
    view.check_reply()?;
    assert_eq!(view_any(&bytes)?, Ok(()));
    let answered = text_bytes("prepare_text_reply")?;
    let reply = PrepareView::from_frame(&frame_of(&answered)?)?;
    assert_eq!(reply.statement(), 2);
    assert_eq!(reply.result_types(), [ColumnType::Node, ColumnType::String]);
    assert_eq!(
        reply.names().iter().collect::<Vec<_>>(),
        ["who", "n", "name"]
    );
    reply.check_reply()?;
    assert_eq!(view_any(&answered)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_pattern_is_of_language_0_and_names_nothing() -> TestResult {
    let bytes = golden("prepare")?.bytes()?;
    let view = PrepareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.language(), Language::Pattern);
    assert_eq!(view.text(), "");
    assert!(view.names().is_empty());
    let named = with_section(&bytes, 10, 5, |ends| {
        ends.extend_from_slice(&1u32.to_le_bytes())
    });
    let named = with_section(&named, 10, 6, |text| text.push(b'n'));
    assert_eq!(
        refused(&named)?,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "names",
            count: 1,
            expected: 0
        }
    );
    Ok(())
}

/// §8.1's order: a pattern's names are refused after the pattern and before sections 5 and 6 are read as a string list;
/// a text's UTF-8 before its names.
#[test]
fn test_names_are_checked_after_the_pattern_or_the_text() -> TestResult {
    let bytes = golden("prepare")?.bytes()?;
    let text_only = with_section(&bytes, 10, 6, |text| text.push(b'n'));
    assert_eq!(
        refused(&text_only)?,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "names",
            count: 1,
            expected: 0
        }
    );
    let broken_pattern = with_section(&bytes, 10, 0, |pattern| pattern.push(0));
    let both = with_section(&broken_pattern, 10, 6, |text| text.push(b'n'));
    assert_eq!(refused(&both)?, refused(&broken_pattern)?);
    let not_utf8 = with_section(&text_bytes("prepare_text")?, 10, 0, |text| text[0] = 0xFF);
    let short_ends = with_section(&not_utf8, 10, 5, |ends| ends.truncate(3));
    assert_eq!(refused(&short_ends)?, refused(&not_utf8)?);
    assert!(matches!(
        refused(&not_utf8)?,
        ProtocolErrorEnum::NotUtf8 { section: 0, .. }
    ));
    Ok(())
}

#[test]
fn test_the_language_is_0_to_3_after_the_unused_bytes() -> TestResult {
    // PROTOCOL.md section 8.1: 0 a quad pattern, 1 GQL, 2 SPARQL, 3 UQL; 4 names no language.
    let bytes = golden("prepare")?.bytes()?;
    let frame = with_command_header(&bytes, |h| h[24] = 4);
    assert_eq!(
        refused(&frame)?,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "language",
            value: 4
        }
    );
    let unused = with_command_header(&bytes, |h| {
        h[24] = 4;
        h[25] = 1;
    });
    assert_eq!(
        refused(&unused)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 25 }
    );
    Ok(())
}

#[test]
fn test_a_text_is_utf8() -> TestResult {
    let frame = with_section(&text_bytes("prepare_text")?, 10, 0, |text| text[0] = 0xFF);
    assert_eq!(
        refused(&frame)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 10,
            section: 0,
            offset: 0
        }
    );
    Ok(())
}

/// A pattern's types are 1 or 2, a text's 1 to 13.
#[test]
fn test_a_column_type_follows_the_language() -> TestResult {
    let pattern_frame = golden("prepare")?.bytes()?;
    let integer = with_section(&pattern_frame, 10, 3, |types| types[0] = 3);
    assert_eq!(
        refused(&integer)?,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 3
        }
    );
    let request = text_bytes("prepare_text")?;
    for number in 1..=13u8 {
        let frame = with_section(&request, 10, 3, |types| types[0] = number);
        assert_eq!(
            PrepareView::from_frame(&frame_of(&frame)?)?.parameter_types()[0].number(),
            number
        );
    }
    for number in [0u8, 14] {
        let frame = with_section(&request, 10, 3, |types| types[0] = number);
        assert_eq!(
            refused(&frame)?,
            ProtocolErrorEnum::FieldValue {
                command: 10,
                field: "column type",
                value: number.into()
            }
        );
    }
    Ok(())
}

#[test]
fn test_a_texts_names_count_its_parameters_and_columns() -> TestResult {
    let request = with_section(&text_bytes("prepare_text")?, 10, 3, Vec::clear);
    assert_eq!(
        refused(&request)?,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "parameter types",
            count: 0,
            expected: 1
        }
    );
    let reply = with_section(&text_bytes("prepare_text_reply")?, 10, 4, |types| {
        types.push(1)
    });
    assert_eq!(
        refused(&reply)?,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "names",
            count: 3,
            expected: 4
        }
    );
    Ok(())
}

/// Stage 6.6: Prepare's ten sections, `language` 3 and its warnings (`PROTOCOL.md` §8.1, §4.9), over Prepares laid out by
/// hand: sections 0 the text, 1 and 2 the term table, 3 and 4 the column types, 5 and 6 the names, 7 to 9 the
/// warnings.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        BodyBuilder, BodyView, ColumnType, Command, Header, Language, Prepare, PrepareView,
        TermTable, Warning,
    };
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::{CLUSTER, frame_of};
    use crate::utils::ugs_frames::{prepare_frame, refused, strings, warning_sections};

    const TEXT: &str = "Company { id }";
    /// UQL0110, a retired name resolved in its deprecation window: language 3, number 110 (§4.9, §9.4).
    const UQL0110: u32 = (3 << 16) | 110;

    /// The ten sections of a text's answer: the text, no term, no parameter, one result column `id` of type 6, and
    /// the warnings `warnings`.
    fn answer_sections(warnings: [Vec<u8>; 3]) -> Vec<Vec<u8>> {
        let [ends, text] = strings(&["id"]);
        let [records, message_ends, message_text] = warnings;
        vec![
            TEXT.as_bytes().to_vec(),
            vec![],
            vec![],
            vec![],
            vec![6],
            ends,
            text,
            records,
            message_ends,
            message_text,
        ]
    }

    /// A UQL text's answer of ten sections, one warning: language 3, the column, the warning read in place.
    #[test]
    fn test_a_uql_answer_reads_its_ten_sections_and_warning() -> TestResult {
        let bytes = prepare_frame(
            2,
            0x5C4E,
            3,
            &answer_sections(warning_sections(&[(UQL0110, 0, "UQL0110 m")])),
        )?;
        let view = PrepareView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.language(), Language::Uql);
        assert_eq!(view.text(), TEXT);
        assert_eq!(
            view.result_types(),
            &[ColumnType::from_number(6).context("type 6")?]
        );
        view.check_reply()?;
        let warning = view.warnings().get(0).context("the warning")?;
        assert_eq!((warning.number(), warning.language()), (110, 3));
        assert_eq!(warning.message(), "UQL0110 m");
        Ok(())
    }

    /// The builder writes the same ten sections, language 3 at command-header offset 24.
    #[test]
    fn test_a_uql_answer_is_built_with_its_warning() -> TestResult {
        let answer = Prepare::of_text(1, Language::Uql, TEXT, TermTable::new(), vec![], vec![])
            .answered(
                2,
                0x5C4E,
                vec![(
                    "id".to_string(),
                    ColumnType::from_number(6).context("type 6")?,
                )],
            )
            .with_warnings(vec![
                Warning::new(UQL0110, "UQL0110 m").with_location(10..17),
            ]);
        let header = Header::new(CLUSTER, Command::Prepare).with_stream(3);
        let expected = prepare_frame(
            2,
            0x5C4E,
            3,
            &answer_sections(warning_sections(&[(UQL0110, 0, "UQL0110 m")])),
        )?;
        assert_eq!(answer.frame(header)?, expected);
        Ok(())
    }

    /// Sections 7 to 9 are empty for a pattern, refused as `CountMismatch` naming `warnings`, and in a request, which
    /// `check_request` refuses so.
    #[test]
    fn test_a_pattern_and_a_request_carry_no_warning() -> TestResult {
        let warnings = || warning_sections(&[(UQL0110, 0, "m")]);
        let count_field = |refusal: &ProtocolErrorEnum| match refusal {
            ProtocolErrorEnum::CountMismatch { command, field, .. } => Some((*command, *field)),
            _ => None,
        };
        // A request of the text: no term, no parameter, no result type and no name, with a warning.
        let [records, message_ends, message_text] = warnings();
        let request = prepare_frame(
            0,
            0,
            3,
            &[
                TEXT.as_bytes().to_vec(),
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                records,
                message_ends,
                message_text,
            ],
        )?;
        let view = PrepareView::from_frame(&frame_of(&request)?)?;
        let refusal = view.check_request().err().context("a refusal")?.0;
        assert_eq!(count_field(&refusal), Some((10, "warnings")), "{refusal:?}");
        // A pattern of one unbound slot answered with a warning: the pattern's bytes from the golden reply.
        let pattern = {
            let golden = crate::utils::golden_bodies::golden("prepare_reply")?.bytes()?;
            let data = crate::utils::golden_bodies::body_data(&golden).to_vec();
            let mut sections = crate::utils::golden_bodies::split_sections(&data, 0, 7);
            sections.extend(warnings());
            prepare_frame(1, 0x5C4E, 0, &sections)?
        };
        let refusal = refused::<PrepareView>(&pattern)?;
        assert_eq!(count_field(&refusal), Some((10, "warnings")), "{refusal:?}");
        Ok(())
    }

    /// Sections 7 to 9 are all empty in a request (§8.1): messages without a warning are refused by `check_request` as
    /// `CountMismatch` naming `warnings`, as a warning is.
    #[test]
    fn test_a_request_holding_messages_alone_is_refused() -> TestResult {
        let [message_ends, message_text] = strings(&["UQL0110 m"]);
        let request = prepare_frame(
            0,
            0,
            3,
            &[
                TEXT.as_bytes().to_vec(),
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                message_ends,
                message_text,
            ],
        )?;
        let view = PrepareView::from_frame(&frame_of(&request)?)?;
        assert!(view.warnings().is_empty());
        let refusal = view.check_request().err().context("a refusal")?.0;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::CountMismatch {
                    command: 10,
                    field: "warnings",
                    ..
                }
            ),
            "{refusal:?}"
        );
        Ok(())
    }
}
