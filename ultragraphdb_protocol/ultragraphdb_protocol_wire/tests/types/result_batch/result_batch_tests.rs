/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ABSENT, BodyBuilder, ResultBatch, ResultTerm};

use crate::utils::golden_bodies::golden;

pub const LONG: &str = "a long lexical form beyond fifteen";

pub fn golden_terms() -> [ResultTerm; 4] {
    let mut big = vec![0x21];
    big.extend_from_slice(Hash128::of(LONG.as_bytes()).as_bytes());
    [
        ResultTerm::new(EncodedTerm::uid(5)).with_text("http://ex.org/a"),
        ResultTerm::new(EncodedTerm::from_bytes(&[0x20, 2, b'A', b'l']).unwrap()),
        ResultTerm::new(EncodedTerm::from_bytes(&big).unwrap()).with_text(LONG),
        ResultTerm::new(EncodedTerm::uid(6)).with_text("http://ex.org/b"),
    ]
}

pub fn golden_batch() -> ResultBatch {
    let [a, al, long, b] = golden_terms();
    let mut batch = ResultBatch::new(2).with_resume(b"rk".to_vec());
    batch.push_row(&[Some(&a), Some(&al)]).unwrap();
    batch.push_row(&[Some(&a), Some(&long)]).unwrap();
    batch.push_row(&[Some(&b), None]).unwrap();
    batch
}

#[test]
fn test_the_golden_batch_builds_the_golden_frame() {
    let batch = golden_batch();
    assert_eq!(batch.columns(), 2);
    assert_eq!(batch.rows(), 3);
    assert_eq!(batch.terms(), &golden_terms());
    assert_eq!(batch.resume(), b"rk");
    assert_eq!(
        batch.frame(golden("result").header()).unwrap(),
        golden("result").bytes()
    );
    assert_eq!(batch.encoded_len(), batch.body().len());
    assert_eq!(batch.encoded_len(), 506 - 256);
}

#[test]
fn test_cells_index_the_term_table_in_order_of_first_use() {
    let batch = golden_batch();
    assert_eq!(batch.cell(0, 0), Some(0));
    assert_eq!(batch.cell(0, 1), Some(0));
    assert_eq!(batch.cell(0, 2), Some(3));
    assert_eq!(batch.cell(1, 0), Some(1));
    assert_eq!(batch.cell(1, 1), Some(2));
    assert_eq!(batch.cell(1, 2), Some(ABSENT));
    assert_eq!(batch.cell(2, 0), None);
    assert_eq!(batch.cell(0, 3), None);
}

#[test]
fn test_a_row_of_the_wrong_width_is_refused() {
    let [a, ..] = golden_terms();
    let mut batch = ResultBatch::new(2);
    assert_eq!(
        batch.push_row(&[Some(&a)]).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "row",
            count: 1,
            expected: 2
        }
    );
    assert_eq!(batch.rows(), 0);
    assert!(batch.push_row(&[None, None]).is_ok());
    let mut none = ResultBatch::new(0);
    assert!(none.push_row(&[]).is_ok());
    assert_eq!(none.rows(), 1);
}
