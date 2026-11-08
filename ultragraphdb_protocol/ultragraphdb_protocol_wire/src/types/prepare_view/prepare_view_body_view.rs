/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::language::Language;
use crate::types::pattern_slot::PatternSlot;
use crate::types::prepare::Prepare;
use crate::types::prepare_view::PrepareView;
use crate::types::quad_pattern::QuadPattern;
use crate::types::string_list::StringList;
use crate::types::term_table_view::TermTableView;
use crate::utils::sections::Sections;
use crate::utils::sections::{expect_command, text};

/// The column types `bytes` hold, each 1 or 2 for a pattern and 1 to 13 for a text, refused as
/// [`ProtocolError::field_value`] (`"column type"`).
fn column_types(
    command: u16,
    language: Language,
    bytes: &[u8],
) -> Result<Vec<ColumnType>, ProtocolError> {
    bytes
        .iter()
        .map(|byte| {
            ColumnType::from_number(*byte)
                .filter(|column| language != Language::Pattern || column.in_pattern())
                .ok_or_else(|| ProtocolError::field_value(command, "column type", (*byte).into()))
        })
        .collect()
}

/// A pattern's types against `pattern`: one parameter type per parameter slot, and in an answer one result type per
/// unbound slot and one more.
fn check_pattern_types(
    command: u16,
    pattern: &QuadPattern,
    parameter_types: &[ColumnType],
    result_types: &[ColumnType],
) -> Result<(), ProtocolError> {
    if parameter_types.len() != pattern.parameters() {
        return Err(ProtocolError::count_mismatch(
            command,
            "parameter types",
            // The casts are lossless: a `usize` is at most 64 bits.
            parameter_types.len() as u64,
            pattern.parameters() as u64,
        ));
    }
    let unbound = pattern
        .slots()
        .iter()
        .filter(|slot| matches!(slot, PatternSlot::Unbound))
        .count();
    // The unbound slots, then the reifier column.
    let columns = unbound + 1;
    if !result_types.is_empty() && result_types.len() != columns {
        return Err(ProtocolError::count_mismatch(
            command,
            "result types",
            // The casts are lossless: a `usize` is at most 64 bits.
            result_types.len() as u64,
            columns as u64,
        ));
    }
    Ok(())
}

/// A text's types against its `names`: in a request one parameter type per name; in an answer, which holds result types,
/// the names counting the parameters and the result columns.
fn check_text_types(
    command: u16,
    names: usize,
    parameters: usize,
    results: usize,
) -> Result<(), ProtocolError> {
    if results == 0 && parameters != names {
        return Err(ProtocolError::count_mismatch(
            command,
            "parameter types",
            // The casts are lossless: a `usize` is at most 64 bits.
            parameters as u64,
            names as u64,
        ));
    }
    let named = parameters.saturating_add(results);
    if results != 0 && names != named {
        return Err(ProtocolError::count_mismatch(
            command,
            "names",
            // The casts are lossless: a `usize` is at most 64 bits.
            names as u64,
            named as u64,
        ));
    }
    Ok(())
}

impl<'a> BodyView<'a> for PrepareView<'a> {
    const COMMAND: Command = Command::Prepare;

    type Body = Prepare;

    /// Checks, in `PROTOCOL.md` §8.1's order: unused bytes 25–127 zero; `language`; the sections; the term table; for a
    /// pattern, the pattern and no names; for a text, the text UTF-8 and the names; the parameter types' bytes, then the
    /// result types', each 1 or 2 for a pattern and 1 to 13 for a text; the counts.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 25)])?;
        let number = header.u8_at(24);
        let language = Language::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "language", number.into()))?;
        let sections = Sections::<7>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let (pattern, query) = if language == Language::Pattern {
            let pattern = QuadPattern::read(command, 0, sections.get(0), terms.len(), true)?;
            let (ends, names_text) = (sections.get(5), sections.get(6));
            if !ends.is_empty() || !names_text.is_empty() {
                // The names `ends` holds, at least one where only the text does.
                let held = ends.len().div_ceil(4).max(1);
                return Err(ProtocolError::count_mismatch(
                    command,
                    "names",
                    held as u64, // lossless: a `usize` is at most 64 bits
                    0,
                ));
            }
            (Some(pattern), "")
        } else {
            (None, text(command, 0, sections.get(0))?)
        };
        let names = StringList::read(command, (5, 6), sections.get(5), sections.get(6))?;
        let parameter_types = column_types(command, language, sections.get(3))?;
        let result_types = column_types(command, language, sections.get(4))?;
        match &pattern {
            Some(pattern) => {
                check_pattern_types(command, pattern, &parameter_types, &result_types)?;
            }
            None => check_text_types(
                command,
                names.len(),
                parameter_types.len(),
                result_types.len(),
            )?,
        }
        Ok(Self {
            tenant: header.u64_at(0),
            statement: header.u64_at(8),
            schema: header.u64_at(16),
            language,
            terms,
            pattern,
            text: query,
            parameter_types,
            result_types,
            names,
        })
    }

    fn to_body(&self) -> Result<Prepare, ProtocolError> {
        if let Some(pattern) = self.pattern {
            return Ok(Prepare::new(
                self.tenant,
                self.statement,
                self.schema,
                self.terms.to_table(),
                pattern,
                self.parameter_types.clone(),
                self.result_types.clone(),
            ));
        }
        let mut names = self.names.iter().map(str::to_string);
        let parameters = names.by_ref().take(self.parameter_types.len()).collect();
        let projections = names.zip(self.result_types.iter().copied()).collect();
        Ok(Prepare::of_text(
            self.tenant,
            self.language,
            self.text,
            self.terms.to_table(),
            parameters,
            self.parameter_types.clone(),
        )
        .answered(self.statement, self.schema, projections))
    }
}
