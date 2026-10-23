/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/chunk.go (DataFormat). Changed for ultragraphdb: translated to Rust; reading
 * and chunking input files belong to the loaders; the format option wins over the extension, as both loaders'
 * --format help states.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::input_format::InputFormat;

/// Dgraph's `DataFormat`: the data format the option names, `rdf` or `json` in any case; with any other option,
/// the format of the file's name, `.gz` ignored: `.rdf` is RDF and `.json` JSON.
///
/// Both loaders' `--format` help gives the option precedence; Dgraph's code lets RDF, by name or option, win over
/// JSON (ledger 21).
pub fn data_format(filename: &str, format: &str) -> InputFormat {
    match format.to_lowercase().as_str() {
        "rdf" => return InputFormat::Rdf,
        "json" => return InputFormat::Json,
        _ => {}
    }
    let filename = filename.to_lowercase();
    let filename = filename.strip_suffix(".gz").unwrap_or(&filename);
    if filename.ends_with(".rdf") {
        InputFormat::Rdf
    } else if filename.ends_with(".json") {
        InputFormat::Json
    } else {
        InputFormat::Unknown
    }
}
