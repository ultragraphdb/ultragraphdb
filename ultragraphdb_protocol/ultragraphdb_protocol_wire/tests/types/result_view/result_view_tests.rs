/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ABSENT, BodyView, ResultBatch, ResultView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::result_batch::result_batch_tests::{LONG, REIFIER, SKOLEM_6, golden_batch};
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

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
    let bytes = with_section(&golden("result")?.bytes()?, 7, index, change);
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
    let mut secs = split_sections(body_data(&bytes), 0, 7);
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
    // Columns 0 and 6: the unbound slots and the reifier column are 1 to 5.
    for columns in [0u64, 6] {
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
    // An epoch past 2^42 - 1; bytes 20 to 127 unused.
    let epoch = with_command_header(&bytes, |h| put(h, 8, 1 << 42, 8));
    assert_eq!(
        refusal(&epoch)?,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "epoch",
            value: 1 << 42
        }
    );
    let unused = with_command_header(&bytes, |h| h[20] = 1);
    assert_eq!(
        refusal(&unused)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 148 }
    );
    // Row 0's reifier cell, cell 6, naming term 0, an IRI; row 2's naming term 3, a literal.
    let iri = with_section(&bytes, 7, 0, |cells| put(cells, 24, 0, 4));
    assert_eq!(
        refusal(&iri)?,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 0
        }
    );
    let literal = with_section(&bytes, 7, 0, |cells| put(cells, 32, 3, 4));
    assert_eq!(
        refusal(&literal)?,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 2
        }
    );
    // A blank node there, the other one, reads.
    let blank = with_section(&bytes, 7, 0, |cells| put(cells, 32, 4, 4));
    assert_eq!(
        ResultView::from_frame(&frame_of(&blank)?)?.cell(2, 2),
        Some(4)
    );
    Ok(())
}

#[test]
fn test_cells_count_rows_times_columns_in_sixty_four_bits() -> TestResult {
    // 4 × 2^30 is 2^32: a 32-bit product would wrap to 0 and match a batch of no cells.
    let wrapped = with_command_header(&golden("result")?.bytes()?, |h| {
        put(h, 0, 4, 4);
        put(h, 4, 1 << 30, 4);
    });
    let empty = with_section(&wrapped, 7, 0, |cells| cells.clear());
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
    let short = with_section(&blank, 7, 2, |e| {
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
    let unknown = with_section(&blank, 7, 2, |e| e[38] = 0x03);
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
    let none = with_section(&bytes, 7, 1, |offsets| offsets.clear());
    let none = with_section(&none, 7, 3, |texts| texts.clear());
    let none = with_section(&none, 7, 0, |cells| cells.fill(0xFF));
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
    let empty = with_section(&none, 7, 2, |e| e.clear());
    let view = ResultView::from_frame(&frame_of(&empty)?)?;
    assert_eq!(view.term_count(), 0);
    assert_eq!(view.cell(1, 2), Some(ABSENT));
    Ok(())
}
