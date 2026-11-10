/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod query_view_body_view;

use ultragraphdb_core_body::{StringList, u32_at};
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::language::Language;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::QUERY_TEXT_RESERVE;

/// The body of a received Query frame, read in place; see [`crate::Query`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryView<'a> {
    transaction: u64,
    language: Language,
    limit: u32,
    text: &'a str,
    names: StringList<'a>,
    values: &'a [u8],
    terms: TermTableView<'a>,
    resume: &'a [u8],
}

impl<'a> QueryView<'a> {
    /// The stream number of the Begin of the transaction the query runs in.
    #[must_use]
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The language of its text: GQL or SPARQL.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// The most rows returned; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The query text, borrowed from the frame.
    #[must_use]
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// The number of parameters.
    #[must_use]
    pub fn parameter_count(&self) -> usize {
        self.names.len()
    }

    /// Parameter `index`: its name and its term's index; `None` past the last.
    #[must_use]
    pub fn parameter(&self, index: usize) -> Option<(&'a str, u32)> {
        Some((self.names.get(index)?, u32_at(self.values, index)?))
    }

    /// The request's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    #[must_use]
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }

    /// The rule a receiver announcing `max_frame` holds the text to: at most that less [`QUERY_TEXT_RESERVE`].
    ///
    /// # Errors
    ///
    /// [`ProtocolError::count_mismatch`] (`"query"`, the text's bytes, the most it may hold) for a longer text.
    pub fn check_text(&self, max_frame: u32) -> Result<(), ProtocolError> {
        let most = u64::from(max_frame.saturating_sub(QUERY_TEXT_RESERVE));
        let bytes = self.text.len() as u64; // lossless: a `usize` is at most 64 bits
        if bytes > most {
            return Err(ProtocolError::count_mismatch(
                Command::Query.number(),
                "query",
                bytes,
                most,
            ));
        }
        Ok(())
    }
}
