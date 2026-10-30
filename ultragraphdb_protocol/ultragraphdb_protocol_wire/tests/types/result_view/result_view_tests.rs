/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ABSENT, BodyView, ResultBatch, ResultView};

use crate::types::result_batch::result_batch_tests::{LONG, SKOLEM_6, golden_batch};
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

#[test]
fn test_the_golden_batch_reads_its_cells_and_terms() {
    let bytes = golden("result").bytes();
    let view = ResultView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!((view.columns(), view.rows(), view.term_count()), (2, 3, 4));
    let cells: Vec<Option<u32>> = (0..2)
        .flat_map(|c| (0..3).map(move |r| (c, r)))
        .map(|(c, r)| view.cell(c, r))
        .collect();
    assert_eq!(
        cells,
        vec![Some(0), Some(0), Some(3), Some(1), Some(2), Some(ABSENT)]
    );
    assert_eq!(view.cell(2, 0), None);
    assert_eq!(view.term(2).unwrap().text(), Some(LONG));
    assert_eq!(view.term(3).unwrap().text(), Some(SKOLEM_6));
    assert!(view.term(3).unwrap().is_blank_node());
    assert!(!view.term(0).unwrap().is_blank_node());
    assert!(view.has_statement_ids());
    assert_eq!(
        (0..4).map(|r| view.statement_id(r)).collect::<Vec<_>>(),
        vec![Some(0x1001), Some(0x1002), Some(0x1003), None]
    );
    assert_eq!(view.term(4), None);
    assert_eq!(view.resume(), b"rk");
    assert_eq!(view.to_body().unwrap(), golden_batch());
}

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = with_section(&golden("result").bytes(), 8, index, change);
    ResultView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

#[test]
fn test_cells_are_counted_and_index_the_table() {
    assert_eq!(
        refused(0, |cells| cells.truncate(20)),
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 5,
            expected: 6
        }
    );
    assert_eq!(
        refused(0, |cells| put(cells, 8, 4, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 13,
            section: 0,
            at: 2,
            index: 4,
            limit: 4
        }
    );
    // Rows and columns past what a body could hold do not overflow.
    let huge = with_command_header(&golden("result").bytes(), |h| {
        put(h, 0, u32::MAX.into(), 4);
        put(h, 4, u32::MAX.into(), 4);
    });
    // (2^32 - 1)^2 = 2^64 - 2^33 + 1 = 18,446,744,065,119,617,025 cells expected; the golden six held.
    assert_eq!(
        ResultView::from_frame(&frame_of(&huge)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 6,
            expected: 18_446_744_065_119_617_025
        }
    );
}

#[test]
fn test_terms_are_encodings_laid_end_to_end() {
    // The first offset not zero; an offset going back; an encoding not of core_terms; one ending past the next.
    assert_eq!(
        refused(1, |offsets| put(offsets, 0, 1, 4)),
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
    assert_eq!(
        refused(1, |offsets| put(offsets, 8, 2, 4)),
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 1
        }
    );
    assert_eq!(
        refused(2, |terms| terms[0] = 0xEE),
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
    assert_eq!(
        refused(2, |terms| terms.truncate(terms.len() - 1)),
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 3
        }
    );
    assert_eq!(
        refused(1, |offsets| put(offsets, 12, 40, 4)),
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 2
        }
    );
    assert_eq!(
        refused(1, |offsets| offsets.truncate(2)),
        ProtocolErrorEnum::SectionLength {
            command: 13,
            section: 1,
            len: 2,
            unit: 4
        }
    );
}

#[test]
fn test_the_first_term_starts_its_section() {
    // A junk byte before the first encoding, every offset moved past it: each term would still parse.
    use crate::utils::golden_bodies::{body_data, join_sections, reframe, split_sections};
    let bytes = golden("result").bytes();
    let mut secs = split_sections(body_data(&bytes), 0, 8);
    secs[1] = secs[1]
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()) + 1)
        .flat_map(|o| o.to_le_bytes())
        .collect();
    secs[2].insert(0, 0x01);
    let shifted = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        ResultView::from_frame(&frame_of(&shifted)).unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 0
        }
    );
}

#[test]
fn test_texts_are_counted_and_in_range() {
    assert_eq!(
        refused(3, |texts| texts.truncate(24)),
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "term texts",
            count: 3,
            expected: 4
        }
    );
    assert_eq!(
        refused(3, |texts| put(texts, 4, 3, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 13,
            section: 3,
            at: 1,
            index: 3,
            limit: 3
        }
    );
}

#[test]
fn test_a_batch_of_vectors_and_long_terms_round_trips() {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultTerm};
    let mut vector = vec![0x40, 3, 0];
    for x in [1.5f32, -2.0, 0.25] {
        vector.extend_from_slice(&x.to_le_bytes());
    }
    let v = ResultTerm::new(EncodedTerm::from_bytes(&vector).unwrap());
    let typed =
        ResultTerm::new(EncodedTerm::from_bytes(&[0x32, 0, 0, 0, 0, 0, 0, 0, 9, 1, b'x']).unwrap())
            .with_annotation("http://ex.org/type");
    let mut batch = ResultBatch::new(1);
    batch.push_row(&[Some(&v)]).unwrap();
    batch.push_row(&[Some(&typed)]).unwrap();
    batch.push_row(&[Some(&v)]).unwrap();
    let bytes = batch.frame(golden("result").header()).unwrap();
    let view = ResultView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.term_count(), 2);
    assert_eq!(
        view.term(1).unwrap().annotation(),
        Some("http://ex.org/type")
    );
    assert_eq!(view.to_body().unwrap(), batch);
}

#[test]
fn test_statement_ids_are_none_or_one_per_row() {
    assert_eq!(
        refused(7, |ids| ids.truncate(16)),
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "statement ids",
            count: 16,
            expected: 24
        }
    );
    assert_eq!(
        refused(7, |ids| ids.push(0)),
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "statement ids",
            count: 25,
            expected: 24
        }
    );
    let none = with_section(&golden("result").bytes(), 8, 7, |ids| ids.clear());
    let view = ResultView::from_frame(&frame_of(&none)).unwrap();
    assert!(!view.has_statement_ids());
    assert_eq!(view.statement_id(0), None);
}

#[test]
fn test_cells_count_rows_times_columns_in_sixty_four_bits() {
    // 65,536 × 65,536 is 2^32: a 32-bit product would wrap to 0 and match a batch of no cells.
    let wrapped = with_command_header(&golden("result").bytes(), |h| {
        put(h, 0, 65_536, 4);
        put(h, 4, 65_536, 4);
    });
    let empty = with_section(&wrapped, 8, 0, |cells| cells.clear());
    let empty = with_section(&empty, 8, 7, |ids| ids.clear());
    assert_eq!(
        ResultView::from_frame(&frame_of(&empty)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "cells",
            count: 0,
            expected: 1 << 32
        }
    );
}

/// A one-term batch of `encoding`, which the store's length rules take, read through the view.
fn one_term(encoding: &[u8]) -> Result<(), ProtocolErrorEnum> {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_protocol_wire::{BodyBuilder, ResultTerm};
    let term = ResultTerm::new(EncodedTerm::from_bytes(encoding).expect("the length rules hold"));
    let mut batch = ResultBatch::new(1);
    batch.push_row(&[Some(&term)]).unwrap();
    let bytes = batch.frame(golden("result").header()).unwrap();
    ResultView::from_frame(&frame_of(&bytes))
        .map(|_| ())
        .map_err(|e| e.0)
}

#[test]
fn test_each_term_is_one_whole_encoding() {
    let bad = Err(ProtocolErrorEnum::BadTerm {
        command: 13,
        section: 2,
        at: 0,
    });
    // PROTOCOL.md section 10.1: inline text UTF-8, a boolean 0 or 1, a known datatype's code listed.
    assert_eq!(one_term(&[0x20, 1, 0xFF]), bad);
    assert_eq!(one_term(&[0x10, 2]), bad);
    assert_eq!(one_term(&[0x10, 0xFF]), bad);
    assert_eq!(one_term(&[0x30, 99, 1, b'x']), bad);
    assert_eq!(one_term(&[0x30, 0, 1, b'x']), bad);
    assert_eq!(one_term(&[0x10, 1]), Ok(()));
    assert_eq!(one_term(&[0x10, 0]), Ok(()));
    assert_eq!(one_term(&[0x30, 33, 1, b'x']), Ok(()));
    assert_eq!(one_term(&[0x20, 2, 0xC3, 0xA9]), Ok(()));
    // The term table's bytes: a blank node's tag with a UID's nine bytes is whole, eight are not.
    let blank = golden("result").bytes();
    assert_eq!(
        ResultView::from_frame(&frame_of(&blank))
            .map(|view| view.term(3).map(|term| term.is_blank_node()))
            .map_err(|e| e.0),
        Ok(Some(true))
    );
    let short = with_section(&blank, 8, 2, |e| {
        e.pop();
    });
    assert_eq!(
        ResultView::from_frame(&frame_of(&short)).unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 3
        }
    );
    // A tag the store never writes, 0x03, is no encoding.
    let unknown = with_section(&blank, 8, 2, |e| e[30] = 0x03);
    assert_eq!(
        ResultView::from_frame(&frame_of(&unknown)).unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 13,
            section: 2,
            at: 3
        }
    );
}

#[test]
fn test_a_read_matching_nothing_is_one_batch_of_no_rows() {
    // Point 9: one batch flagged LAST, `rows` 0, `columns` the unbound slots, no term and no statement ID.
    use ultragraphdb_protocol_wire::{BodyBuilder, FLAG_LAST, HeaderView};
    let batch = ResultBatch::new(3);
    let header = golden("result").header().with_flags(FLAG_LAST);
    let bytes = batch.frame(header).unwrap();
    let frame = frame_of(&bytes);
    assert_eq!(HeaderView::new(&bytes).unwrap().flags(), FLAG_LAST);
    let view = ResultView::from_frame(&frame).unwrap();
    assert_eq!((view.columns(), view.rows(), view.term_count()), (3, 0, 0));
    assert_eq!(view.cell(0, 0), None);
    assert!(!view.has_statement_ids());
    assert!(view.resume().is_empty());
    assert_eq!(view.to_body().unwrap(), batch);
}

#[test]
fn test_a_batch_of_no_terms_holds_no_encodings() {
    // PROTOCOL.md section 9.1: with no terms, section 2 is empty (`CountMismatch` `"term encodings"`). The golden
    // batch with its offsets (1) and texts (3) emptied and every cell (0) unbound: its encodings (2) remain.
    let bytes = golden("result").bytes();
    let encodings = crate::utils::golden_bodies::split_sections(
        crate::utils::golden_bodies::body_data(&bytes),
        0,
        8,
    )[2]
    .len();
    assert!(encodings > 0);
    let none = with_section(&bytes, 8, 1, |offsets| offsets.clear());
    let none = with_section(&none, 8, 3, |texts| texts.clear());
    let none = with_section(&none, 8, 0, |cells| cells.fill(0xFF));
    assert_eq!(
        ResultView::from_frame(&frame_of(&none)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "term encodings",
            count: encodings as u64,
            expected: 0
        }
    );
    // With the encodings emptied as well, the batch reads: its six cells unbound.
    let empty = with_section(&none, 8, 2, |e| e.clear());
    let view = ResultView::from_frame(&frame_of(&empty)).unwrap();
    assert_eq!(view.term_count(), 0);
    assert_eq!(view.cell(1, 2), Some(ABSENT));
}
