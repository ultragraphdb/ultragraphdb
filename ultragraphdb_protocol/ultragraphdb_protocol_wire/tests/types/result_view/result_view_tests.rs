/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyBuilder, BodyView, FLAG_LAST, Monotonic, PATTERN_COLUMNS_MAX, QUERY_COLUMNS_MAX,
    ResultBatch, ResultView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::result_batch::result_batch_tests::{LONG, REIFIER, SKOLEM_6, golden_batch};
use crate::utils::golden_bodies::{
    frame_of, golden, put, view_any, with_command_header, with_section,
};

#[test]
fn test_the_golden_batch_reads_its_cells_and_terms() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.columns(), view.rows(), view.term_count()), (3, 3, 5));
    assert_eq!((view.epoch(), view.partition()), (30, 2));
    let cells: Vec<Option<u32>> = (0..3)
        .flat_map(|c| (0..3).map(move |r| (c, r)))
        .map(|(c, r)| view.cell(c, r))
        .collect();
    assert_eq!(
        cells,
        vec![
            Some(0),
            Some(0),
            Some(4),
            Some(1),
            Some(3),
            Some(ABSENT),
            Some(2),
            Some(ABSENT),
            Some(ABSENT)
        ]
    );
    assert_eq!(view.cell(3, 0), None);
    let term = |i: u32| view.term(i).context("a term of the batch");
    assert_eq!(term(2)?.text(), Some(REIFIER));
    assert!(term(2)?.is_blank_node());
    assert_eq!(term(3)?.text(), Some(LONG));
    assert_eq!(term(4)?.text(), Some(SKOLEM_6));
    assert!(term(4)?.is_blank_node());
    assert!(!term(0)?.is_blank_node());
    assert_eq!(view.term(5), None);
    assert_eq!(view.resume(), b"rk");
    assert_eq!(view.to_body()?, golden_batch()?);
    Ok(())
}

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_section(&golden("result")?.bytes()?, 12, index, change);
    Ok(ResultView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_cells_are_counted_and_index_the_table() -> TestResult {
    assert_eq!(
        refused(0, |cells| cells.truncate(32))?,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 8,
            expected: 9
        }
    );
    assert_eq!(
        refused(0, |cells| put(cells, 8, 5, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 13,
            section: 0,
            at: 2,
            index: 5,
            limit: 5
        }
    );
    // Rows and columns past what a body could hold do not overflow.
    let huge = with_command_header(&golden("result")?.bytes()?, |h| {
        put(h, 0, 5, 4);
        put(h, 4, u32::MAX.into(), 4);
    });
    // 5 × (2^32 − 1) cells expected, past a u32; the golden nine held.
    assert_eq!(
        ResultView::from_frame(&frame_of(&huge)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 9,
            expected: 5 * 4_294_967_295
        }
    );
    Ok(())
}

#[test]
fn test_terms_are_encodings_laid_end_to_end() -> TestResult {
    // The first offset not zero; an offset going back; an encoding not of core_terms; one ending past the next.
    assert_eq!(
        refused(1, |offsets| put(offsets, 0, 1, 4))?,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
    assert_eq!(
        refused(1, |offsets| put(offsets, 8, 2, 4))?,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 1
        }
    );
    assert_eq!(
        refused(2, |terms| terms[0] = 0xEE)?,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
    assert_eq!(
        refused(2, |terms| terms.truncate(terms.len() - 1))?,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 4
        }
    );
    assert_eq!(
        refused(1, |offsets| put(offsets, 12, 40, 4))?,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 2
        }
    );
    assert_eq!(
        refused(1, |offsets| offsets.truncate(2))?,
        ProtocolErrorEnum::SectionLength {
            command: 13,
            section: 1,
            len: 2,
            unit: 4
        }
    );
    Ok(())
}

#[test]
fn test_the_first_term_starts_its_section() -> TestResult {
    // A junk byte before the first encoding, every offset moved past it: each term would still parse.
    use crate::utils::golden_bodies::{body_data, join_sections, reframe, split_sections};
    let bytes = golden("result")?.bytes()?;
    let mut secs = split_sections(body_data(&bytes), 0, 12);
    secs[1] = secs[1]
        .chunks(4)
        .map(|c| c.try_into().map(|o| u32::from_le_bytes(o) + 1))
        .collect::<Result<Vec<u32>, _>>()?
        .into_iter()
        .flat_map(|o| o.to_le_bytes())
        .collect();
    secs[2].insert(0, 0x01);
    let shifted = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        ResultView::from_frame(&frame_of(&shifted)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
    Ok(())
}

#[test]
fn test_texts_are_counted_and_in_range() -> TestResult {
    assert_eq!(
        refused(3, |texts| texts.truncate(24))?,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "term texts",
            count: 3,
            expected: 5
        }
    );
    assert_eq!(
        refused(3, |texts| put(texts, 4, 4, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 13,
            section: 3,
            at: 1,
            index: 4,
            limit: 4
        }
    );
    Ok(())
}

#[test]
fn test_a_batch_of_vectors_and_long_terms_round_trips() -> TestResult {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultTerm};
    let mut vector = vec![0x40, 3, 0];
    for x in [1.5f32, -2.0, 0.25] {
        vector.extend_from_slice(&x.to_le_bytes());
    }
    let v = ResultTerm::new(EncodedTerm::from_bytes(&vector)?);
    // Tag 0x32: the datatype's 16 bytes, then the inline lexical form `x`.
    let mut typed = vec![0x32];
    typed.extend_from_slice(ultragraphdb_core_terms::Hash128::of(b"http://ex.org/type").as_bytes());
    typed.extend_from_slice(&[1, b'x']);
    let typed =
        ResultTerm::new(EncodedTerm::from_bytes(&typed)?).with_annotation("http://ex.org/type");
    let mut batch = ResultBatch::new(2);
    batch.push_row(&[Some(&v), None])?;
    batch.push_row(&[Some(&typed), None])?;
    batch.push_row(&[Some(&v), None])?;
    let bytes = batch.frame(golden("result")?.header())?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.term_count(), 2);
    assert_eq!(
        view.term(1).context("term 1 of the batch")?.annotation(),
        Some("http://ex.org/type")
    );
    assert_eq!(view.to_body()?, batch);
    Ok(())
}

#[test]
fn test_header_fields_and_the_reifier_column_are_checked() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let refusal = |frame: &[u8]| -> TestResult<ProtocolErrorEnum> {
        Ok(ResultView::from_frame(&frame_of(frame)?)
            .err()
            .context("a refusal")?
            .0)
    };
    // Columns 1,025, and 0 with rows: a batch holds at most a query's projections.
    for columns in [0u64, u64::from(QUERY_COLUMNS_MAX) + 1] {
        let frame = with_command_header(&bytes, |h| put(h, 0, columns, 4));
        assert_eq!(
            refusal(&frame)?,
            ProtocolErrorEnum::FieldValue {
                command: 13,
                field: "columns",
                value: columns
            }
        );
    }
    // An epoch past 2^42 - 1; bytes 21 to 127 unused.
    let epoch = with_command_header(&bytes, |h| put(h, 8, 1 << 42, 8));
    assert_eq!(
        refusal(&epoch)?,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "epoch",
            value: 1 << 42
        }
    );
    let unused = with_command_header(&bytes, |h| h[21] = 1);
    assert_eq!(
        refusal(&unused)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 149 }
    );
    Ok(())
}

/// What a receiver asking a quad pattern holds the batch to, which its bytes alone do not tell from a query's.
#[test]
fn test_a_patterns_batch_holds_its_columns_and_reifier_column() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let refusal = |frame: &[u8]| -> TestResult<ProtocolErrorEnum> {
        Ok(ResultView::from_frame(&frame_of(frame)?)?
            .check_pattern()
            .err()
            .context("a refusal")?
            .0)
    };
    for columns in [0, PATTERN_COLUMNS_MAX + 1] {
        let wide = ResultBatch::new(columns).frame(golden("result")?.header())?;
        assert_eq!(
            refusal(&wide)?,
            ProtocolErrorEnum::FieldValue {
                command: 13,
                field: "columns",
                value: columns.into()
            }
        );
    }
    ResultView::from_frame(&frame_of(&bytes)?)?.check_pattern()?;
    // Row 0's reifier cell, cell 6, naming term 0, an IRI: a declared relation's handle IRI, tag 0x01, is taken
    // (PROTOCOL.md section 9.1); row 2's naming term 3, a literal, is not.
    let iri = with_section(&bytes, 12, 0, |cells| put(cells, 24, 0, 4));
    ResultView::from_frame(&frame_of(&iri)?)?.check_pattern()?;
    let literal = with_section(&bytes, 12, 0, |cells| put(cells, 32, 3, 4));
    assert_eq!(
        refusal(&literal)?,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 2
        }
    );
    // A blank node there, the other one, reads.
    let blank = with_section(&bytes, 12, 0, |cells| put(cells, 32, 4, 4));
    let read = ResultView::from_frame(&frame_of(&blank)?)?;
    read.check_pattern()?;
    assert_eq!(read.cell(2, 2), Some(4));
    Ok(())
}

#[test]
fn test_cells_count_rows_times_columns_in_sixty_four_bits() -> TestResult {
    // 4 × 2^30 is 2^32: a 32-bit product would wrap to 0 and match a batch of no cells.
    let wrapped = with_command_header(&golden("result")?.bytes()?, |h| {
        put(h, 0, 4, 4);
        put(h, 4, 1 << 30, 4);
    });
    let empty = with_section(&wrapped, 12, 0, |cells| cells.clear());
    assert_eq!(
        ResultView::from_frame(&frame_of(&empty)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 0,
            expected: 1 << 32
        }
    );
    Ok(())
}

/// A one-term batch of `encoding`, which the store's length rules take, read through the view; the outer result fails
/// the test when the batch cannot be built.
fn one_term(encoding: &[u8]) -> TestResult<Result<(), ProtocolErrorEnum>> {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultTerm};
    let term = ResultTerm::new(EncodedTerm::from_bytes(encoding).context("the length rules hold")?);
    let mut batch = ResultBatch::new(2);
    batch.push_row(&[Some(&term), None])?;
    let bytes = batch.frame(golden("result")?.header())?;
    Ok(ResultView::from_frame(&frame_of(&bytes)?)
        .map(|_| ())
        .map_err(|e| e.0))
}

#[test]
fn test_each_term_is_one_whole_encoding() -> TestResult {
    let bad = Err(ProtocolErrorEnum::BadTerm {
        command: 13,
        section: 2,
        at: 0,
    });
    // PROTOCOL.md section 10.1: inline text UTF-8, a boolean 0 or 1, a known datatype's code listed.
    assert_eq!(one_term(&[0x20, 1, 0xFF])?, bad);
    assert_eq!(one_term(&[0x10, 2])?, bad);
    assert_eq!(one_term(&[0x10, 0xFF])?, bad);
    assert_eq!(one_term(&[0x30, 99, 1, b'x'])?, bad);
    assert_eq!(one_term(&[0x30, 0, 1, b'x'])?, bad);
    assert_eq!(one_term(&[0x10, 1])?, Ok(()));
    assert_eq!(one_term(&[0x10, 0])?, Ok(()));
    assert_eq!(one_term(&[0x30, 33, 1, b'x'])?, Ok(()));
    assert_eq!(one_term(&[0x20, 2, 0xC3, 0xA9])?, Ok(()));
    // The term table's bytes: a blank node's tag with its 16-byte hash is whole, 15 bytes are not.
    let blank = golden("result")?.bytes()?;
    assert_eq!(
        ResultView::from_frame(&frame_of(&blank)?)
            .map(|view| view.term(4).map(|term| term.is_blank_node()))
            .map_err(|e| e.0),
        Ok(Some(true))
    );
    let short = with_section(&blank, 12, 2, |e| {
        assert!(e.pop().is_some(), "the section holds a byte to cut");
    });
    assert_eq!(
        ResultView::from_frame(&frame_of(&short)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 4
        }
    );
    // A tag the store never writes, 0x03, is no encoding: term 3 starts after 17 + 4 + 17 bytes.
    let unknown = with_section(&blank, 12, 2, |e| e[38] = 0x03);
    assert_eq!(
        ResultView::from_frame(&frame_of(&unknown)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 3
        }
    );
    Ok(())
}

#[test]
fn test_a_read_matching_nothing_is_one_batch_of_no_rows() -> TestResult {
    // One batch flagged LAST, `rows` 0, `columns` the unbound slots and the reifier column, no term.
    use ultragraphdb_protocol_wire::{BodyBuilder, FLAG_LAST, HeaderView};
    let batch = ResultBatch::new(3);
    let header = golden("result")?.header().with_flags(FLAG_LAST);
    let bytes = batch.frame(header)?;
    let frame = frame_of(&bytes)?;
    assert_eq!(HeaderView::new(&bytes)?.flags(), FLAG_LAST);
    let view = ResultView::from_frame(&frame)?;
    assert_eq!((view.columns(), view.rows(), view.term_count()), (3, 0, 0));
    assert_eq!(view.cell(0, 0), None);
    assert!(view.resume().is_empty());
    assert_eq!(view.to_body()?, batch);
    Ok(())
}

#[test]
fn test_a_batch_of_no_terms_holds_no_encodings() -> TestResult {
    // PROTOCOL.md section 9.1: with no terms, section 2 is empty (`CountMismatch` `"term encodings"`). The golden
    // batch with its offsets (1) and texts (3) emptied and every cell (0) unbound: its encodings (2) remain.
    let bytes = golden("result")?.bytes()?;
    let encodings = crate::utils::golden_bodies::split_sections(
        crate::utils::golden_bodies::body_data(&bytes),
        0,
        7,
    )[2]
    .len();
    assert!(encodings > 0);
    let none = with_section(&bytes, 12, 1, |offsets| offsets.clear());
    let none = with_section(&none, 12, 3, |texts| texts.clear());
    let none = with_section(&none, 12, 0, |cells| cells.fill(0xFF));
    assert_eq!(
        ResultView::from_frame(&frame_of(&none)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "term encodings",
            count: encodings as u64,
            expected: 0
        }
    );
    // With the encodings emptied as well, the batch reads: its nine cells unbound.
    let empty = with_section(&none, 12, 2, |e| e.clear());
    let view = ResultView::from_frame(&frame_of(&empty)?)?;
    assert_eq!(view.term_count(), 0);
    assert_eq!(view.cell(1, 2), Some(ABSENT));
    Ok(())
}

#[test]
fn test_the_golden_querys_batch_reads_its_names_and_flag() -> TestResult {
    let bytes = golden("result_query")?.bytes()?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.monotonic(), Monotonic::Retractable);
    let names: Vec<&str> = view.column_names().iter().collect();
    assert_eq!(names, ["n", "age"]);
    assert_eq!(view_any(&bytes)?, Ok(()));
    // Its last column holds a null here; an IRI there, as a pattern's reifier column holds a handle IRI (section 9.1),
    // is taken by a pattern's rule too.
    let iri = with_section(&bytes, 12, 0, |cells| put(cells, 4, 0, 4));
    let read = ResultView::from_frame(&frame_of(&iri)?)?;
    assert_eq!(read.cell(1, 0), Some(0));
    read.check_pattern()?;
    Ok(())
}

#[test]
fn test_the_monotonic_byte_is_0_to_2_after_the_epoch() -> TestResult {
    let bytes = golden("result")?.bytes()?;
    for value in 0..=2u8 {
        let frame = with_command_header(&bytes, |h| h[20] = value);
        assert_eq!(
            ResultView::from_frame(&frame_of(&frame)?)?
                .monotonic()
                .number(),
            value
        );
    }
    let refused_frame = with_command_header(&bytes, |h| h[20] = 3);
    assert_eq!(
        ResultView::from_frame(&frame_of(&refused_frame)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "monotonic",
            value: 3
        }
    );
    let epoch = with_command_header(&bytes, |h| {
        h[20] = 3;
        put(h, 8, 1 << 42, 8);
    });
    assert!(matches!(
        ResultView::from_frame(&frame_of(&epoch)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue { field: "epoch", .. }
    ));
    Ok(())
}

#[test]
fn test_column_names_are_empty_or_one_per_column() -> TestResult {
    let bytes = golden("result_query")?.bytes()?;
    let empty = with_section(&bytes, 12, 7, Vec::clear);
    let empty = with_section(&empty, 12, 8, Vec::clear);
    assert!(
        ResultView::from_frame(&frame_of(&empty)?)?
            .column_names()
            .is_empty()
    );
    let extra = with_section(&bytes, 12, 7, |ends| {
        let last = ends[ends.len() - 4..].to_vec();
        ends.extend_from_slice(&last);
    });
    assert_eq!(
        ResultView::from_frame(&frame_of(&extra)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "column names",
            count: 3,
            expected: 2
        }
    );
    Ok(())
}

#[test]
fn test_a_resume_key_stays_off_every_batch_but_the_last() -> TestResult {
    let header = golden("result")?.header().with_flags(0);
    let keyed = ResultBatch::new(1).with_resume(b"rk".to_vec());
    assert_eq!(
        ResultView::from_frame(&frame_of(&keyed.frame(header.clone())?)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "resume key",
            value: 2
        }
    );
    assert!(
        ResultView::from_frame(&frame_of(&ResultBatch::new(1).frame(header.clone())?)?).is_ok()
    );
    let last = keyed.frame(header.with_flags(FLAG_LAST))?;
    assert_eq!(ResultView::from_frame(&frame_of(&last)?)?.resume(), b"rk");
    Ok(())
}

/// Each row of `PROTOCOL.md` §10.3's table, a term of the tag with what it names, broken one index at a time.
#[test]
fn test_a_terms_text_and_annotation_follow_its_tag() -> TestResult {
    let hash = [7u8; 16];
    let cases: [(Vec<u8>, bool, bool); 8] = [
        ([&[0x01], &hash[..]].concat(), true, false),
        ([&[0x02], &hash[..]].concat(), true, false),
        ([&[0x21], &hash[..]].concat(), true, false),
        ([&[0x23], &hash[..], &[0][..]].concat(), true, false),
        ([&[0x24], &hash[..], &hash[..]].concat(), true, true),
        ([&[0x32], &hash[..], &[0][..]].concat(), false, true),
        ([&[0x33], &hash[..], &hash[..]].concat(), true, true),
        (vec![0x20, 0], false, false),
    ];
    let bytes = golden("result")?.bytes()?;
    for (encoding, text, annotation) in cases {
        let batch = |texts: [u32; 2]| -> Vec<u8> {
            let one = with_section(&bytes, 12, 0, |cells| {
                *cells = [
                    0u32, ABSENT, ABSENT, ABSENT, ABSENT, ABSENT, ABSENT, ABSENT, ABSENT,
                ]
                .iter()
                .flat_map(|cell| cell.to_le_bytes())
                .collect();
            });
            let one = with_section(&one, 12, 1, |offsets| *offsets = vec![0; 4]);
            let one = with_section(&one, 12, 2, |terms| terms.clone_from(&encoding));
            with_section(&one, 12, 3, |pairs| {
                *pairs = texts.iter().flat_map(|index| index.to_le_bytes()).collect();
            })
        };
        let string = |named: bool| if named { 0 } else { ABSENT };
        let held = batch([string(text), string(annotation)]);
        assert!(
            ResultView::from_frame(&frame_of(&held)?).is_ok(),
            "{encoding:02x?}"
        );
        for broken in [
            [string(!text), string(annotation)],
            [string(text), string(!annotation)],
        ] {
            assert_eq!(
                ResultView::from_frame(&frame_of(&batch(broken))?)
                    .err()
                    .context("a refusal")?
                    .0,
                ProtocolErrorEnum::FieldValue {
                    command: 13,
                    field: "term text",
                    value: 0
                },
                "{encoding:02x?}"
            );
        }
    }
    Ok(())
}

/// A GQL statement without `RETURN` answers one batch of no column and no row.
#[test]
fn test_a_batch_of_no_column_holds_no_row() -> TestResult {
    let header = golden("result")?.header();
    let bytes = ResultBatch::new(0).frame(header)?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.columns(), view.rows()), (0, 0));
    assert_eq!(view.cell(0, 0), None);
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// A query's batch holds 1 to 1,024 columns (`PROTOCOL.md` §9.1): 1,024 is read, 1,025 refused.
#[test]
fn test_a_batch_of_the_most_columns_is_read() -> TestResult {
    let header = golden("result")?.header();
    let bytes = ResultBatch::new(1_024).frame(header.clone())?;
    let view = ResultView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.columns(), view.rows()), (1_024, 0));
    let past = ResultBatch::new(1_025).frame(header)?;
    assert_eq!(
        ResultView::from_frame(&frame_of(&past)?)
            .map(|_| ())
            .map_err(|e| e.0),
        Err(ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "columns",
            value: 1_025
        })
    );
    Ok(())
}

/// Stage 6.6: a Result's twelve sections, its warnings and its reifier column's handle IRIs (`PROTOCOL.md` §9.1, §4.9,
/// §10.2), over batches laid out by hand.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{BodyView, ResultView};
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::frame_of;
    use crate::utils::ugs_frames::{
        UGS0902, named, no_rest, outcome, refused, result_frame, warning_sections,
    };

    /// `https://w3id.org/ultragraphdb/ns/tenant/0/` then `MemberOf/` and a handle, a declared relation's instance by
    /// its handle IRI (`SCHEMA.md` section 7.3): bucket 0x00002A in 6 digits and 104 bits in 26.
    const HANDLE: &str =
        "https://w3id.org/ultragraphdb/ns/tenant/0/MemberOf/00002a-0000000000000000000000000b";

    /// A read that matches nothing: one batch flagged `LAST`, `rows` 0, two columns, twelve empty sections.
    #[test]
    fn test_an_empty_batch_holds_twelve_sections() -> TestResult {
        let bytes = result_frame(2, 0, &[], &[], no_rest(), true)?;
        let view = ResultView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!((view.columns(), view.rows(), view.term_count()), (2, 0, 0));
        assert!(view.warnings().is_empty());
        view.check_pattern()?;
        Ok(())
    }

    /// A body of nine sections, the layout before stage 6.6, is too short for twelve: 72 bytes, the table needing 96.
    #[test]
    fn test_a_batch_of_nine_sections_is_refused() -> TestResult {
        let nine = crate::utils::ugs_frames::frame(
            ultragraphdb_protocol_wire::Command::Result,
            &[(0, &2u32.to_le_bytes())],
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
            true,
        )?;
        assert_eq!(
            refused::<ResultView>(&nine)?,
            ProtocolErrorEnum::BodyTooShort {
                command: 13,
                len: 72,
                need: 96
            }
        );
        Ok(())
    }

    /// wire-protocol, "A handle IRI is taken in the reifier column": a pattern's batch whose reifier cell names a term
    /// of tag `0x01` is read; one of tag `0x30` is refused as `FieldValue` naming `reifier`, the row.
    #[test]
    fn test_a_handle_iri_is_taken_in_the_reifier_column() -> TestResult {
        let handle = result_frame(1, 1, &[0], &[named(0x01, HANDLE)], no_rest(), true)?;
        ResultView::from_frame(&frame_of(&handle)?)?.check_pattern()?;
        // `0x30`, a literal of the known datatype `xsd:integer` (code 2), lexical form `7` inline.
        let literal = result_frame(
            1,
            1,
            &[0],
            &[(vec![0x30, 2, 1, b'7'], None)],
            no_rest(),
            true,
        )?;
        let view = ResultView::from_frame(&frame_of(&literal)?)?;
        assert_eq!(
            view.check_pattern().err().context("a refusal")?.0,
            ProtocolErrorEnum::FieldValue {
                command: 13,
                field: "reifier",
                value: 0
            }
        );
        Ok(())
    }

    /// A query's first batch carries its compilation's warnings in sections 9 to 11, each of source 0; another source
    /// is refused as `FieldValue` naming `source`.
    #[test]
    fn test_a_querys_first_batch_carries_its_warnings_of_source_0() -> TestResult {
        let [records, ends, text] = warning_sections(&[(UGS0902, 0, "m")]);
        let bytes = result_frame(
            0,
            0,
            &[],
            &[],
            [vec![], vec![], vec![], records, ends, text],
            true,
        )?;
        let view = ResultView::from_frame(&frame_of(&bytes)?)?;
        let warning = view.warnings().get(0).context("the warning")?;
        assert_eq!((warning.number(), warning.message()), (902, "m"));
        let [records, ends, text] = warning_sections(&[(UGS0902, 1, "m")]);
        let sourced = result_frame(
            0,
            0,
            &[],
            &[],
            [vec![], vec![], vec![], records, ends, text],
            true,
        )?;
        assert_eq!(
            refused::<ResultView>(&sourced)?,
            ProtocolErrorEnum::FieldValue {
                command: 13,
                field: "source",
                value: 1
            }
        );
        assert_eq!(outcome::<ResultView>(&bytes)?, None);
        Ok(())
    }

    /// One term of a query's batch, one column and one row.
    fn one_term(encoding: Vec<u8>) -> TestResult<Vec<u8>> {
        result_frame(1, 1, &[0], &[(encoding, None)], no_rest(), true)
    }

    fn bad_term() -> ProtocolErrorEnum {
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0,
        }
    }

    /// The datatypes 34 to 36 (§10.1, §10.4, §10.5): a `bfloat16vector` by tag `0x41`, a dimension u16 little-endian
    /// of at least 1 and two bytes per element; a `measure` by `0x50`, an f64 big-endian and its unit's code inline, 1
    /// to 15 bytes of UTF-8; a `precision` by `0x51`, a byte 1 to 12; each code in tags `0x30` and `0x31`.
    #[test]
    fn test_the_datatypes_34_to_36_are_whole_terms() -> TestResult {
        // 1.0 and -2.0 as bfloat16, the high 16 bits of binary32: 0x3F80 and 0xC000, little-endian.
        let vector = vec![0x41, 2, 0, 0x80, 0x3F, 0x00, 0xC0];
        // 1.5 as binary64 big-endian, 0x3FF8000000000000, then the unit `m`.
        let measure = [vec![0x50], 1.5f64.to_be_bytes().to_vec(), vec![1, b'm']].concat();
        let accepted = [
            vector,
            measure,
            vec![0x51, 1],
            vec![0x51, 12],
            vec![0x30, 34, 3, b'[', b'1', b']'],
            vec![0x30, 35, 5, b'1', b'.', b'5', b' ', b'm'],
            vec![0x30, 36, 4, b'y', b'e', b'a', b'r'],
        ];
        for encoding in accepted {
            assert_eq!(
                outcome::<ResultView>(&one_term(encoding.clone())?)?,
                None,
                "{encoding:?}"
            );
        }
        let long_unit = [
            vec![0x50],
            1.5f64.to_be_bytes().to_vec(),
            vec![16],
            vec![b'u'; 16],
        ]
        .concat();
        let refused_terms = [
            vec![0x41, 0, 0],
            vec![0x41, 2, 0, 0x80, 0x3F, 0x00],
            [vec![0x50], 1.5f64.to_be_bytes().to_vec(), vec![0]].concat(),
            long_unit,
            [vec![0x50], 1.5f64.to_be_bytes().to_vec(), vec![1, 0xFF]].concat(),
            vec![0x51, 0],
            vec![0x51, 13],
            vec![0x30, 37, 1, b'x'],
        ];
        for encoding in refused_terms {
            assert_eq!(
                refused::<ResultView>(&one_term(encoding.clone())?)?,
                bad_term(),
                "{encoding:?}"
            );
        }
        Ok(())
    }
}
