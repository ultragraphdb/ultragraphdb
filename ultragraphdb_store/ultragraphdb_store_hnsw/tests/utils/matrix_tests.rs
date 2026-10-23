/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_store_hnsw::{decode_uint64_matrix, encode_uint64_matrix};
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

#[derive(Debug, Deserialize)]
struct MatrixCase {
    name: String,
    matrix: Vec<Vec<u64>>,
    encoded: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn test_the_matrix_encodes_as_gos_and_decodes_back() {
    let golden = load_golden::<MatrixCase>(&testdata_dir().join("golden/hnsw_matrix.json"));
    assert_eq!(golden.cases().len(), 14);
    for c in golden.cases() {
        let encoded = encode_uint64_matrix(&c.matrix);
        assert_eq!(hex(&encoded), c.encoded, "{}", c.name);
        assert_eq!(
            decode_uint64_matrix(&encoded),
            Ok(c.matrix.clone()),
            "{}",
            c.name
        );
    }
}
