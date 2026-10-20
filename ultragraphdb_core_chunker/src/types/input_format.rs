/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/chunk.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The formats of the bulk and live loaders' input, with Dgraph's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InputFormat {
    /// A format the loaders do not take.
    Unknown = 0,
    /// RDF N-Quads.
    Rdf = 1,
    /// JSON.
    Json = 2,
}
