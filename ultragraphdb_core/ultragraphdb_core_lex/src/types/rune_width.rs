/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A run of `count` consecutive reads of runes `width` bytes wide, so that several backups in a row undo the right
/// number of bytes. For example, after reading runes of 1, 1 and 3 bytes the stack holds (1, 2) and (3, 1), and
/// three backups move back by 3, 1 and 1. A read at the end of the input pushes width 0.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RuneWidth {
    pub width: usize,
    /// At least 1: an entry is popped when its count would reach 0.
    pub count: usize,
}
