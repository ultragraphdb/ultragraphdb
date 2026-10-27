/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::sections::{as_u32, u32_at, u32s};

/// A list of strings read in place from two sections: the end offset of each string in the text, little-endian u32s,
/// then the text. The ends never decrease, the last is the text's length, and every string is UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringList<'a> {
    ends: &'a [u8],
    text: &'a str,
}

impl<'a> StringList<'a> {
    /// The list of `ends` and `text`, sections `sections.0` and `sections.1` of a body of `command`, checked in order:
    /// `ends` whole u32s ([`ProtocolError::SectionLength`]); each end at least the one before and at most the text's
    /// length ([`ProtocolError::IndexOutOfRange`]); the last end the text's length ([`ProtocolError::CountMismatch`]);
    /// each string UTF-8: the text UTF-8 and every end on a character boundary ([`ProtocolError::NotUtf8`], at the
    /// first invalid byte or at the end inside a character).
    pub(crate) fn read(
        command: u16,
        sections: (usize, usize),
        ends: &'a [u8],
        text: &'a [u8],
    ) -> Result<Self, ProtocolError> {
        let values = u32s(command, sections.0, ends)?;
        let limit = as_u32(text.len());
        let mut previous = 0u32;
        for (at, end) in values.iter().enumerate() {
            if *end < previous || *end > limit {
                return Err(ProtocolError::IndexOutOfRange(
                    command, sections.0, at, *end, limit,
                ));
            }
            previous = *end;
        }
        if previous != limit {
            return Err(ProtocolError::CountMismatch(
                command,
                "string text",
                text.len() as u64,
                previous.into(),
            ));
        }
        let text = std::str::from_utf8(text)
            .map_err(|e| ProtocolError::NotUtf8(command, sections.1, e.valid_up_to()))?;
        if let Some(end) = values
            .iter()
            .find(|end| !text.is_char_boundary(**end as usize))
        {
            return Err(ProtocolError::NotUtf8(command, sections.1, *end as usize));
        }
        Ok(Self { ends, text })
    }

    pub fn len(&self) -> usize {
        self.ends.len() / 4
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// String `index`, `None` past the last.
    pub fn get(&self, index: usize) -> Option<&'a str> {
        let end = u32_at(self.ends, index)? as usize;
        let start = match index {
            0 => 0,
            _ => u32_at(self.ends, index - 1)? as usize,
        };
        self.text.get(start..end)
    }

    /// The strings in order.
    pub fn iter(&self) -> impl Iterator<Item = &'a str> + 'a {
        let list = *self;
        (0..list.len()).filter_map(move |i| list.get(i))
    }
}
