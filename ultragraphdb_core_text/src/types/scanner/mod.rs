/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/parse.go (type scanner). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::language_error::LanguageError;

/// Scans BCP 47 tokens, separated by '-' (or '_', converted on construction), rewriting its buffer in place as
/// the parser normalizes the tag.
///
/// The buffer mirrors Go's byte slice over its backing array: removing bytes shifts the rest left in place and
/// leaves the old bytes beyond the new length, and the current token is a view of the array at the position it
/// was scanned at. Go's parser reads the token after shifting the buffer in a few places, and sees exactly
/// those bytes; so does this port.
#[derive(Debug, Clone)]
pub(crate) struct Scanner {
    /// The backing array; the buffer is its first `len` bytes.
    arr: Vec<u8>,
    len: usize,
    /// The current token as a range of the backing array at the time it was scanned; `None` once the input is
    /// exhausted.
    token: Option<(usize, usize)>,
    /// Start position of the current token.
    pub start: usize,
    /// End position of the current token.
    pub end: usize,
    /// Next point for scan.
    pub next: usize,
    pub err: Option<LanguageError>,
    pub done: bool,
}

impl Scanner {
    /// A scanner over `b`, positioned on its first token.
    pub(crate) fn new(mut b: Vec<u8>) -> Scanner {
        for c in b.iter_mut() {
            if *c == b'_' {
                *c = b'-';
            }
        }
        let len = b.len();
        let mut s = Scanner {
            arr: b,
            len,
            token: None,
            start: 0,
            end: 0,
            next: 0,
            err: None,
            done: false,
        };
        s.scan();
        s
    }

    /// The buffer.
    pub(crate) fn b(&self) -> &[u8] {
        &self.arr[..self.len]
    }

    /// The buffer, mutable.
    pub(crate) fn b_mut(&mut self) -> &mut [u8] {
        &mut self.arr[..self.len]
    }

    /// The length of the buffer.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Shortens the buffer to `n` bytes, keeping the bytes beyond in the backing array.
    pub(crate) fn truncate(&mut self, n: usize) {
        self.len = self.len.min(n);
    }

    /// Appends `bytes` to the buffer, overwriting the backing array beyond it.
    pub(crate) fn append(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        if end > self.arr.len() {
            self.arr.resize(end, 0);
        }
        self.arr[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }

    /// Go's `copy(b[p:], src)`: copies as much of `src` as fits before the end of the buffer.
    pub(crate) fn copy_at(&mut self, p: usize, src: &[u8]) {
        let n = src.len().min(self.len.saturating_sub(p));
        self.arr[p..p + n].copy_from_slice(&src[..n]);
    }

    /// Removes `[start, end)` from the buffer in place: the rest moves left and the old bytes stay beyond the new
    /// length.
    fn remove(&mut self, start: usize, end: usize) {
        self.arr.copy_within(end..self.len, start);
        self.len -= end - start;
    }

    /// The current token, empty when there is none.
    pub(crate) fn token(&self) -> &[u8] {
        match self.token {
            Some((s, e)) => &self.arr[s..e],
            None => &[],
        }
    }

    /// The current token, mutable, for the lookups that fix its case in place.
    pub(crate) fn token_mut(&mut self) -> &mut [u8] {
        match self.token {
            Some((s, e)) => &mut self.arr[s..e],
            None => &mut [],
        }
    }

    /// The length of the current token, zero when there is none.
    pub(crate) fn token_len(&self) -> usize {
        self.token.map_or(0, |(s, e)| e - s)
    }

    /// Converts `b[start..end]` to lower case; an empty or inverted range is left alone, as Go's loop does.
    pub(crate) fn lower_case(&mut self, start: usize, end: usize) {
        if start < end {
            self.arr[start..end].make_ascii_lowercase();
        }
    }

    /// Records `e` unless an error is already recorded; a syntax error replaces any other.
    pub(crate) fn set_error(&mut self, e: Option<LanguageError>) {
        let replace = match (&self.err, &e) {
            (None, _) => true,
            (Some(old), Some(new)) => new.is_syntax() && !old.is_syntax(),
            (Some(_), None) => false,
        };
        if replace {
            self.err = e;
        }
    }

    /// Shrinks or grows the buffer at `old_start` so that `new_size` bytes fit between `old_start` and `old_end`,
    /// and sets the scan point after the resized range.
    pub(crate) fn resize_range(&mut self, old_start: usize, old_end: usize, new_size: usize) {
        self.start = old_start;
        let end = old_start + new_size;
        if end != old_end {
            let n = self.len + end - old_end;
            if n > self.arr.len() {
                self.arr.resize(n, 0);
            }
            self.arr.copy_within(old_end..self.len, end);
            self.len = n;
            self.next = end + self.next - self.end;
            self.end = end;
        }
    }

    /// Replaces the current token with `repl`.
    pub(crate) fn replace(&mut self, repl: &[u8]) {
        self.resize_range(self.start, self.end, repl.len());
        self.copy_at(self.start, repl);
    }

    /// Removes the current token from the input; the caller must scan next.
    pub(crate) fn gobble(&mut self, e: Option<LanguageError>) {
        self.set_error(e);
        if self.start == 0 {
            self.remove(0, self.next);
            self.end = 0;
        } else {
            self.remove(self.start - 1, self.end);
            self.end = self.start - 1;
        }
        self.next = self.start;
    }

    /// Removes `b[start..end]`, which lies before the current token.
    pub(crate) fn delete_range(&mut self, start: usize, end: usize) {
        self.remove(start, end);
        let diff = end - start;
        self.next -= diff;
        self.start -= diff;
        self.end -= diff;
    }

    /// Scans the next token. Tokens longer than eight characters or with characters other than ASCII letters and
    /// digits are an error and are removed. Returns the end position of the last token consumed.
    pub(crate) fn scan(&mut self) -> usize {
        let end = self.end;
        self.token = None;
        self.start = self.next;
        while self.next < self.len {
            let i = match self.b()[self.next..].iter().position(|&c| c == b'-') {
                None => {
                    self.end = self.len;
                    self.next = self.len;
                    self.end - self.start
                }
                Some(i) => {
                    self.end = self.next + i;
                    self.next = self.end + 1;
                    i
                }
            };
            let token = &self.arr[self.start..self.end];
            if !(1..=8).contains(&i) || !token.iter().all(u8::is_ascii_alphanumeric) {
                self.gobble(Some(LanguageError::syntax()));
                continue;
            }
            self.token = Some((self.start, self.end));
            return end;
        }
        if let Some(&b'-') = self.b().last() {
            self.set_error(Some(LanguageError::syntax()));
            self.len -= 1;
        }
        self.done = true;
        end
    }

    /// Scans tokens of at least `min` bytes. Returns the end position of the last token consumed.
    pub(crate) fn accept_min_size(&mut self, min: usize) -> usize {
        let mut end = self.end;
        self.scan();
        while self.token_len() >= min {
            end = self.end;
            self.scan();
        }
        end
    }
}
