/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/chunk_test.go (TestDataFormat). Changed for ultragraphdb: translated to Rust;
 * the format comes from the name alone, so no files are created.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_chunker::{InputFormat, data_format};

#[test]
fn test_data_format() {
    let expected_outcomes = [
        InputFormat::Json,
        InputFormat::Rdf,
        InputFormat::Unknown,
        InputFormat::Json,
        InputFormat::Rdf,
    ];
    let file_data = [
        "test-1.json",
        "test-2.rdf",
        "test-3.txt",
        "test-4.json.gz",
        "test-5.rdf.gz",
    ];
    for (data, expected) in file_data.iter().zip(expected_outcomes) {
        let file_path = format!("test-files/{data}");
        assert_eq!(data_format(&file_path, ""), expected, "{data}");
    }
}

#[test]
fn the_extension_wins_over_the_format_option_except_rdf_over_json() {
    assert_eq!(data_format("a.RDF", "json"), InputFormat::Rdf);
    // Dgraph's comment gives the extension precedence, but its code tests for RDF, name or option, first.
    assert_eq!(data_format("a.Json.GZ", "rdf"), InputFormat::Rdf);
    assert_eq!(data_format("a.Json.GZ", "other"), InputFormat::Json);
    assert_eq!(data_format("a.txt", "RDF"), InputFormat::Rdf);
    assert_eq!(data_format("a.txt", "Json"), InputFormat::Json);
    assert_eq!(data_format("a.gz", ""), InputFormat::Unknown);
    assert_eq!(data_format("a.rdf.gz.gz", ""), InputFormat::Unknown);
}

#[test]
fn the_formats_keep_dgraph_numbers() {
    assert_eq!(InputFormat::Unknown as u8, 0);
    assert_eq!(InputFormat::Rdf as u8, 1);
    assert_eq!(InputFormat::Json as u8, 2);
}
