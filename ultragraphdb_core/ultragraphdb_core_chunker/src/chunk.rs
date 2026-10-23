/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/chunk.go (DataFormat). Changed for ultragraphdb: translated to Rust; reading
 * and chunking input files belong to the loaders.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::input_format::InputFormat;

/// Dgraph's `DataFormat`: a file's data format from its name, `.gz` ignored, or else from the format option. An
/// `.rdf` name or the `rdf` option is RDF, then a `.json` name or the `json` option is JSON: the extension wins
/// except that the `rdf` option beats a `.json` name.
pub fn data_format(filename: &str, format: &str) -> InputFormat {
    let format = format.to_lowercase();
    let filename = filename.to_lowercase();
    let filename = filename.strip_suffix(".gz").unwrap_or(&filename);
    if filename.ends_with(".rdf") || format == "rdf" {
        InputFormat::Rdf
    } else if filename.ends_with(".json") || format == "json" {
        InputFormat::Json
    } else {
        InputFormat::Unknown
    }
}
