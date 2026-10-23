/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Lexer;

impl<'a> Lexer<'a> {
    /// Reads runes while `c` accepts them, stopping before the first it rejects or the end of the input; the last
    /// rune accepted, `None` when none was.
    pub fn accept_run(&mut self, mut c: impl FnMut(char) -> bool) -> Option<char> {
        let mut last = None;
        while let Some(r) = self.next().filter(|&r| c(r)) {
            last = Some(r);
        }
        self.backup();
        last
    }

    /// As [`Lexer::accept_run`], with a check that may read ahead from the lexer itself.
    pub fn accept_run_rec(&mut self, mut c: impl FnMut(char, &mut Lexer<'a>) -> bool) {
        while let Some(r) = self.next() {
            if !c(r, self) {
                break;
            }
        }
        self.backup();
    }

    /// Reads runes until `c` accepts one or the input ends, stopping before it.
    pub fn accept_until(&mut self, mut c: impl FnMut(char) -> bool) {
        while self.next().is_some_and(|r| !c(r)) {}
        self.backup();
    }

    /// Reads up to `times` runes while `c` accepts them; how many it accepted. As in Go, the last rune read is
    /// backed up even when all `times` were accepted.
    pub fn accept_run_times(&mut self, mut c: impl FnMut(char) -> bool, times: usize) -> usize {
        let mut i = 0;
        while i < times && self.next().is_some_and(&mut c) {
            i += 1;
        }
        self.backup();
        i
    }

    /// Skips the runes `c` accepts.
    pub fn ignore_run(&mut self, c: impl FnMut(char) -> bool) {
        self.accept_run(c);
        self.ignore();
    }
}
