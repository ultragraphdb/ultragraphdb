/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/rdf_parser.go and chunker/json_parser.go. Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use crate::errors::chunker_error::{ChunkerError, ChunkerErrorEnum};

impl fmt::Display for ChunkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ChunkerErrorEnum::Empty => f.write_str("RDF: harmless error, e.g. comment line"),
            ChunkerErrorEnum::Lex(e) => e.fmt(f),
            ChunkerErrorEnum::Rdf(msg) | ChunkerErrorEnum::JsonMutation(msg) => f.write_str(msg),
            ChunkerErrorEnum::Json(e) => e.fmt(f),
            ChunkerErrorEnum::Types(e) => e.fmt(f),
            ChunkerErrorEnum::EmptyUid => {
                f.write_str("UID must be present and non-zero while deleting edges")
            }
        }
    }
}
