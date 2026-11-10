/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation,
    Describe, DescribeView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::describe::describe_tests::golden_answer;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, join_sections, put, reframe, split_sections, with_section,
};

fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

#[test]
fn test_the_golden_answer_reads_its_values() -> TestResult {
    let bytes = golden("describe_reply")?.bytes()?;
    let view = DescribeView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.tenant(), view.read_version()), (0, 42));
    assert_eq!(
        view.labels().collect::<Vec<_>>(),
        vec!["http://ex.org/Person"]
    );
    assert_eq!(
        view.relationship_types().collect::<Vec<_>>(),
        vec!["http://ex.org/knows"]
    );
    assert_eq!(
        view.property_keys().collect::<Vec<_>>(),
        vec!["http://ex.org/name"]
    );
    assert_eq!(view.declarations(), 3);
    assert_eq!(view.strings().len(), 4);
    assert_eq!(view.to_body()?, golden_answer());
    let request = golden("describe")?.bytes()?;
    let request_view = DescribeView::from_frame(&frame_of(&request)?)?;
    assert_eq!(request_view.to_body()?, Describe::request(0));
    assert_eq!(request_view.declarations(), 0);
    Ok(())
}

#[test]
fn test_every_declaration_round_trips() -> TestResult {
    let answer = Describe::new(5, 6)
        .with_predicates(vec![
            DeclaredPredicate::new(ex("a"))
                .with_kind(DeclaredKind::Either)
                .with_single(true)
                .with_datatype("http://www.w3.org/2001/XMLSchema#integer")
                .with_unique(true)
                .with_count_index(true)
                .with_tokenizers(vec!["int".to_string()])
                .with_domains(vec![ex("T"), ex("U")])
                .with_vector_index("hnsw", vec![("metric".to_string(), "cosine".to_string())]),
        ])
        .with_entity_types(vec![
            (DeclaredEntityType::new(ex("T"), Some(ex("U")), true), 0),
            (DeclaredEntityType::new(ex("U"), None, false), 0),
        ])
        .with_relations(vec![DeclaredRelation::new(ex("R"), vec![ex("x"), ex("y")])]);
    let bytes = answer.frame(golden("describe")?.header())?;
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes)?)?.to_body()?,
        answer
    );
    Ok(())
}

#[test]
fn test_only_declarations_and_names_in_range_are_read() -> TestResult {
    let bytes = with_section(&golden("describe_reply")?.bytes()?, 9, 2, |labels| {
        put(labels, 0, 4, 4)
    });
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 12,
            section: 2,
            at: 0,
            index: 4,
            limit: 4
        }
    );
    // Relationship types (section 3) and property keys (4) index the four strings as labels do.
    for section in [3, 4] {
        let indexed = with_section(&golden("describe_reply")?.bytes()?, 9, section, |names| {
            put(names, 0, 4, 4)
        });
        assert_eq!(
            DescribeView::from_frame(&frame_of(&indexed)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::IndexOutOfRange {
                command: 12,
                section,
                at: 0,
                index: 4,
                limit: 4
            },
            "section {section}"
        );
    }
    let bad_record = with_section(&golden("describe_reply")?.bytes()?, 9, 5, |records| {
        records[0] = 1
    });
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bad_record)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 12,
            field: "change kind",
            value: 1
        }
    );
    // A name is text: no IRI syntax is checked here.
    let answer = Describe::new(0, 0).with_labels(vec!["not an iri".to_string()]);
    let answered = answer.frame(golden("describe")?.header())?;
    assert_eq!(
        DescribeView::from_frame(&frame_of(&answered)?)?.to_body()?,
        answer
    );
    Ok(())
}

#[test]
fn test_declarations_come_predicates_first_then_entity_types_then_relations() -> TestResult {
    let answer = Describe::new(0, 0)
        .with_predicates(vec![DeclaredPredicate::new(ex("p"))])
        .with_entity_types(vec![(DeclaredEntityType::new(ex("T"), None, false), 0)]);
    let bytes = answer.frame(golden("describe")?.header())?;
    let mut secs = split_sections(body_data(&bytes), 0, 9);
    // Records 0 (the predicate, args 0..4) and 1 (the entity type, args 4..6) swapped, their args with them.
    let mut entity = secs[5][16..].to_vec();
    let mut predicate = secs[5][..16].to_vec();
    let args = secs[6]
        .chunks(4)
        .map(|c| c.try_into().map(u32::from_le_bytes))
        .collect::<Result<Vec<u32>, _>>()?;
    entity[4..8].copy_from_slice(&0u32.to_le_bytes());
    predicate[4..8].copy_from_slice(&2u32.to_le_bytes());
    secs[5] = [entity, predicate].concat();
    secs[6] = args[4..]
        .iter()
        .chain(&args[..4])
        .flat_map(|a| a.to_le_bytes())
        .collect();
    let swapped = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        DescribeView::from_frame(&frame_of(&swapped)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 12,
            field: "declaration order",
            value: 8
        }
    );
    Ok(())
}

#[test]
fn test_a_nonzero_tenant_and_read_version_read_back() -> TestResult {
    // The golden answer's tenant is 0; an answer built with tenant 5 and read version 6 reads both back.
    let bytes = Describe::new(5, 6).frame(golden("describe")?.header())?;
    let view = DescribeView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.tenant(), view.read_version()), (5, 6));
    Ok(())
}

/// Stage 6: Describe's nine sections and its entries' Order (`PROTOCOL.md` §8.3), over answers laid out by hand.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{BodyView, Command, DescribeView};
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::{body_data, frame_of, section};
    use crate::utils::ugs_frames::{
        EntryRow, describe_frame, entry_sections, frame, outcome, refused, retired, strings, u32s,
    };

    fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
        ProtocolErrorEnum::FieldValue {
            command: 12,
            field,
            value,
        }
    }

    fn answer(names: &[&str], entries: &[EntryRow<'_>]) -> TestResult<Vec<u8>> {
        describe_frame(names, entry_sections(entries), true)
    }

    /// The request: nine empty sections, a body of 72 bytes, every `offset` 72 and every `len` 0.
    #[test]
    fn test_a_request_of_nine_empty_sections_is_read() -> TestResult {
        let request = frame(
            Command::Describe,
            &[],
            &[
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ],
            false,
        )?;
        assert_eq!(body_data(&request).len(), 72);
        let view = DescribeView::from_frame(&frame_of(&request)?)?;
        assert_eq!((view.tenant(), view.read_version()), (0, 0));
        assert_eq!(view.entry_count(), 0);
        assert_eq!(view.declarations(), 0);
        Ok(())
    }

    /// Every kind 1 to 20, one entry each, in kind order: kinds 1 to 18 opaque bytes, 19 UTF-8, 20 a retired name.
    #[test]
    fn test_entries_of_every_kind_are_read() -> TestResult {
        let gone = retired(1, 2, u32::MAX, 1, 0);
        let opaque = [7u8, 7];
        let entries: Vec<EntryRow<'_>> = (1u8..=20)
            .map(|kind| match kind {
                19 => (kind, 0, &b"s"[..]),
                20 => (kind, 0, gone.as_slice()),
                _ => (kind, 0, &opaque[..]),
            })
            .collect();
        let view_bytes = answer(&["n"], &entries)?;
        let view = DescribeView::from_frame(&frame_of(&view_bytes)?)?;
        assert_eq!(view.entry_count(), 20);
        let kinds: Vec<u8> = view.entries().map(|e| e.kind().number()).collect();
        assert_eq!(kinds, (1u8..=20).collect::<Vec<u8>>());
        Ok(())
    }

    /// `kind` is 1 to 20; bytes 1 to 3 zero, refused at the body offset; `name` below the string count; the section a
    /// whole number of 16-byte entries.
    #[test]
    fn test_an_entrys_kind_padding_name_and_record_size_are_checked() -> TestResult {
        for kind in [0u8, 21] {
            assert_eq!(
                refused::<DescribeView>(&answer(&["n"], &[(kind, 0, b"x")])?)?,
                field_value("entry kind", kind.into())
            );
        }
        let [mut records, bytes] = entry_sections(&[(3, 0, b"x")]);
        if let Some(byte) = records.get_mut(2) {
            *byte = 1;
        }
        let padded = describe_frame(&["n"], [records, bytes], true)?;
        let (start, _) = section(body_data(&padded), 0, 7);
        assert_eq!(
            refused::<DescribeView>(&padded)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: 12,
                offset: start + 2
            }
        );
        assert_eq!(
            refused::<DescribeView>(&answer(&["n"], &[(3, 1, b"x")])?)?,
            ProtocolErrorEnum::IndexOutOfRange {
                command: 12,
                section: 7,
                at: 0,
                index: 1,
                limit: 1
            }
        );
        let [mut short, bytes] = entry_sections(&[(3, 0, b"x")]);
        short.truncate(15);
        assert_eq!(
            refused::<DescribeView>(&describe_frame(&["n"], [short, bytes], true)?)?,
            ProtocolErrorEnum::SectionLength {
                command: 12,
                section: 7,
                len: 15,
                unit: 16
            }
        );
        Ok(())
    }

    /// An entry starts where the bytes before end, lies within section 8, and the entries' bytes end with section 8.
    #[test]
    fn test_entries_lie_end_to_end_within_section_8() -> TestResult {
        let [mut records, bytes] = entry_sections(&[(3, 0, b"ab"), (3, 1, b"cd")]);
        // Entry 1's offset at 1, inside entry 0's bytes.
        if let Some(byte) = records.get_mut(16 + 8) {
            *byte = 1;
        }
        let refusal =
            refused::<DescribeView>(&describe_frame(&["a", "b"], [records, bytes], true)?)?;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::FieldValue {
                    command: 12,
                    field: "entry offset",
                    ..
                }
            ),
            "{refusal:?}"
        );
        let [mut long, bytes] = entry_sections(&[(3, 0, b"ab")]);
        if let Some(byte) = long.get_mut(12) {
            *byte = 3;
        }
        let refusal = refused::<DescribeView>(&describe_frame(&["a"], [long, bytes], true)?)?;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::IndexOutOfRange {
                    command: 12,
                    section: 8,
                    at: 0,
                    ..
                }
            ),
            "{refusal:?}"
        );
        let [records, mut bytes] = entry_sections(&[(3, 0, b"ab")]);
        bytes.push(b'c');
        let refusal = refused::<DescribeView>(&describe_frame(&["a"], [records, bytes], true)?)?;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::CountMismatch {
                    command: 12,
                    field: "entry bytes",
                    ..
                }
            ),
            "{refusal:?}"
        );
        Ok(())
    }

    /// Entries come by kind, then by name's UTF-8 bytes, no two alike; a refusal names the entry out of order.
    #[test]
    fn test_entries_come_by_kind_then_by_name() -> TestResult {
        let names = ["a", "b"];
        assert_eq!(
            refused::<DescribeView>(&answer(&names, &[(5, 1, b"x"), (5, 0, b"y")])?)?,
            field_value("entry order", 1)
        );
        assert_eq!(
            refused::<DescribeView>(&answer(&names, &[(5, 0, b"x"), (5, 0, b"y")])?)?,
            field_value("entry order", 1)
        );
        assert_eq!(
            refused::<DescribeView>(&answer(&names, &[(6, 0, b"x"), (5, 1, b"y")])?)?,
            field_value("entry order", 1)
        );
        // By name bytes: "B" (0x42) before "a" (0x61), and a lower kind before a higher whatever the names.
        assert_eq!(
            outcome::<DescribeView>(&answer(&["a", "B"], &[(5, 1, b"x"), (5, 0, b"y")])?)?,
            None
        );
        assert_eq!(
            outcome::<DescribeView>(&answer(&names, &[(5, 1, b"x"), (6, 0, b"y")])?)?,
            None
        );
        Ok(())
    }

    /// A source's text is UTF-8, refused in section 8.
    #[test]
    fn test_a_sources_text_is_utf8() -> TestResult {
        let refusal = refused::<DescribeView>(&answer(&["s"], &[(19, 0, &[b'a', 0xFF])])?)?;
        assert!(
            matches!(
                refusal,
                ProtocolErrorEnum::NotUtf8 {
                    command: 12,
                    section: 8,
                    ..
                }
            ),
            "{refusal:?}"
        );
        Ok(())
    }

    /// A source's text is refused at its first invalid byte, as section 8 counts it (§4.1, §8.3): the source after a
    /// type's 4 bytes, its third byte invalid, is refused at 4 + 2.
    #[test]
    fn test_a_sources_invalid_byte_is_named_at_its_section_offset() -> TestResult {
        let refusal = refused::<DescribeView>(&answer(
            &["T", "s"],
            &[(3, 0, b"abcd"), (19, 1, &[b'a', b'b', 0xFF])],
        )?)?;
        assert_eq!(
            refusal,
            ProtocolErrorEnum::NotUtf8 {
                command: 12,
                section: 8,
                offset: 6
            }
        );
        Ok(())
    }

    /// A retired name is 24 bytes, bytes 2 and 3 zero, `item` 1 to 5, `fate` 1 or 2, `current` a string for fate 1 and
    /// `ABSENT` for 2, `version` at least 1 and `until` 0 for fate 2; each break refused as `FieldValue` naming `retired
    /// name`, the entry.
    #[test]
    fn test_a_retired_names_fields_are_checked() -> TestResult {
        let names = ["Company", "Org"];
        let valid = [retired(1, 1, 1, 3, 9), retired(5, 2, u32::MAX, 4, 0)];
        for bytes in &valid {
            assert_eq!(
                outcome::<DescribeView>(&answer(&names, &[(20, 0, bytes)])?)?,
                None
            );
        }
        let mut long = retired(1, 1, 1, 3, 9);
        long.push(0);
        let mut padded = retired(1, 1, 1, 3, 9);
        if let Some(byte) = padded.get_mut(3) {
            *byte = 1;
        }
        let broken = [
            retired(1, 1, 1, 3, 9)[..23].to_vec(),
            long,
            padded,
            retired(0, 1, 1, 3, 9),
            retired(6, 1, 1, 3, 9),
            retired(1, 0, 1, 3, 9),
            retired(1, 3, 1, 3, 9),
            retired(1, 1, u32::MAX, 3, 9),
            retired(1, 1, 2, 3, 9),
            retired(1, 2, 1, 3, 0),
            retired(1, 1, 1, 0, 9),
            retired(1, 2, u32::MAX, 4, 9),
        ];
        for bytes in &broken {
            assert_eq!(
                refused::<DescribeView>(&answer(&names, &[(20, 0, bytes)])?)?,
                field_value("retired name", 0),
                "{bytes:?}"
            );
        }
        Ok(())
    }

    /// A declaration's `extra` is 0 (§4.6): an entity type's preorder number, sent before stage 6, is refused. The
    /// record of kind 9 laid out by hand: args 0 and 1, the type's IRI and `ABSENT` for no parent, `extra` 1.
    #[test]
    fn test_an_entity_types_extra_is_zero() -> TestResult {
        let [ends, text] = strings(&["http://ex.org/T"]);
        let records = [vec![9, 0, 0, 0], u32s(&[0, 2, 1])].concat();
        let bytes = frame(
            Command::Describe,
            &[(8, &42u64.to_le_bytes())],
            &[
                ends,
                text,
                vec![],
                vec![],
                vec![],
                records,
                u32s(&[0, u32::MAX]),
                vec![],
                vec![],
            ],
            true,
        )?;
        assert_eq!(refused::<DescribeView>(&bytes)?, field_value("extra", 1));
        Ok(())
    }

    /// The view reads its entries back into the owned answer.
    #[test]
    fn test_entries_read_back_into_the_answer() -> TestResult {
        let bytes = answer(&["gen5"], &[(19, 0, b"namespace gen5 {}")])?;
        let body = DescribeView::from_frame(&frame_of(&bytes)?)?.to_body()?;
        let entry = body.entries().first().context("the entry")?;
        assert_eq!(entry.name(), "gen5");
        assert_eq!(entry.bytes(), b"namespace gen5 {}");
        Ok(())
    }
}
