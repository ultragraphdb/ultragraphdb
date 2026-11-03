/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_store_hnsw::{decode_id_matrix, encode_id_matrix};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

use crate::utils::helpers::id;

#[derive(Debug, Deserialize)]
struct MatrixCase {
    name: String,
    matrix: Vec<Vec<u64>>,
}

fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2).context("two digits")?, 16).context("hex"))
        .collect()
}

/// The matrix's bytes written from the layout, apart from the encoder: the row count, then per row its length, each
/// a little-endian u64, and its IDs, each the 16 little-endian bytes of its 128-bit value; no rows, no bytes.
fn layout(matrix: &[Vec<SystemId>]) -> Vec<u8> {
    if matrix.is_empty() {
        return Vec::new();
    }
    let mut out = (matrix.len() as u64).to_le_bytes().to_vec();
    for row in matrix {
        out.extend_from_slice(&(row.len() as u64).to_le_bytes());
        row.iter()
            .for_each(|id| out.extend_from_slice(&id.to_u128().to_le_bytes()));
    }
    out
}

/// Go's matrices (`tools/golden`, `hnsw_matrix`), their uids the IDs numbered by them. Go's encodings are retired, its
/// u64 words being no system IDs, and so is the case `rows`, which holds uid 0: no ID stands for it.
#[test]
fn test_gos_matrices_encode_to_the_id_layout_and_decode_back() -> TestResult {
    let golden = load_golden::<MatrixCase>(&testdata_dir()?.join("golden/hnsw_matrix.json"))?;
    assert_eq!(golden.cases().len(), 14);
    let mut retired = Vec::new();
    for c in golden.cases() {
        if c.matrix.iter().flatten().any(|&uid| uid == 0) {
            retired.push(c.name.as_str());
            continue;
        }
        let matrix: Vec<Vec<SystemId>> = c
            .matrix
            .iter()
            .map(|row| Ok(row.iter().map(|&n| id(n)).collect::<Result<_, _>>()?))
            .collect::<TestResult<_>>()?;
        let encoded = encode_id_matrix(&matrix);
        assert_eq!(encoded, layout(&matrix), "{}", c.name);
        assert_eq!(decode_id_matrix(&encoded), Ok(matrix), "{}", c.name);
    }
    assert_eq!(retired, vec!["rows"]);
    Ok(())
}

#[test]
fn test_a_matrix_has_its_hand_written_bytes() -> TestResult {
    // [[1, 2], []]: 2 rows; a row of 2; the ID numbered 1 little-endian (reserved bytes 0-2 and the bucket's 3-5
    // zero, the sequence 1 in byte 6, the type 1 in byte 14); the ID numbered 2; a row of 0.
    let golden = unhex(concat!(
        "0200000000000000",
        "0200000000000000",
        "00000000000001000000000000000100",
        "00000000000002000000000000000100",
        "0000000000000000",
    ))?;
    let matrix = vec![vec![id(1)?, id(2)?], Vec::new()];
    assert_eq!(encode_id_matrix(&matrix), golden);
    assert_eq!(decode_id_matrix(&golden), Ok(matrix));
    assert_eq!(encode_id_matrix(&[]), Vec::<u8>::new());
    assert_eq!(decode_id_matrix(&[]), Ok(Vec::new()));
    Ok(())
}
