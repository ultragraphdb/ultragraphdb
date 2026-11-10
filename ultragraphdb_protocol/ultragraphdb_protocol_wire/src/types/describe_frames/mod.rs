/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::describe_view::DescribeView;
use crate::types::frame_view::FrameView;
use crate::utils::body::{read_sections, records, text};
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::constants::{CATALOG_BYTES_MAX, COMMAND_HEADER_SIZE, FLAG_LAST};

/// A Describe body's sections.
const SECTIONS: usize = 9;

/// The sections a frame holds whole units of, each with its unit: the u32s of 0, 2, 3, 4 and 6, the 16-byte records of
/// 5 and 7.
const UNITS: [(usize, usize); 7] = [(0, 4), (2, 4), (3, 4), (4, 4), (5, 16), (6, 4), (7, 16)];

/// A Describe answer's frames read as one body (`PROTOCOL.md` §8.3, *Continuation*): an answer larger than one frame
/// within the client's `max_frame` continues in further Describe frames on its stream, each with the first's command
/// header, the last alone flagged `LAST`; each of the body's nine sections is the concatenation, in frame order, of
/// that section's bytes in every frame, and the answer's Order is checked once, on that body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescribeFrames {
    command_header: Option<[u8; COMMAND_HEADER_SIZE]>,
    sections: [Vec<u8>; SECTIONS],
    body: Vec<u8>,
    last: bool,
    frames: u64,
}

impl DescribeFrames {
    /// No frame taken yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes `frame`, the answer's next frame, checked in order: its command Describe
    /// ([`ProtocolError::wrong_command`]); no frame taken flagged `LAST`, and its command header the first frame's
    /// ([`ProtocolError::field_value`] `"describe"`, naming the frames taken); its body nine sections in the layout of
    /// §4.1; sections 0, 2, 3, 4 and 6 whole u32s and sections 5 and 7 whole 16-byte records
    /// ([`ProtocolError::section_length`]); section 1 UTF-8 ([`ProtocolError::not_utf8`]); the answer's sections, this
    /// frame's with them, at most [`CATALOG_BYTES_MAX`] bytes together ([`ProtocolError::field_value`] `"describe"`,
    /// naming their bytes), so assembling an answer is bounded.
    ///
    /// # Errors
    ///
    /// The first rule the frame breaks, in the order above.
    pub fn push(&mut self, frame: &FrameView<'_>) -> Result<(), ProtocolError> {
        let command = Command::Describe.number();
        let header = expect_command(frame, Command::Describe)?;
        let other = self.command_header.is_some_and(|first| first != *header.0);
        if self.last || other {
            return Err(ProtocolError::field_value(command, "describe", self.frames));
        }
        let sections = read_sections::<SECTIONS>(command, frame.body(), 0)?;
        for (section, unit) in UNITS {
            let _whole = records(command, section, sections.get(section), unit)?;
        }
        let _text: &str = text(command, 1, sections.get(1))?;
        let held = (0..SECTIONS)
            .map(|section| sections.get(section).len())
            .chain(self.sections.iter().map(Vec::len))
            .fold(0usize, usize::saturating_add);
        if held > CATALOG_BYTES_MAX {
            // Lossless: a `usize` is at most 64 bits.
            return Err(ProtocolError::field_value(command, "describe", held as u64));
        }
        for (into, section) in self.sections.iter_mut().zip(0..SECTIONS) {
            into.extend_from_slice(sections.get(section));
        }
        self.command_header = Some(*header.0);
        self.frames = self.frames.saturating_add(1);
        self.last = frame.header().flags() & FLAG_LAST != 0;
        if self.last {
            let parts: Vec<&[u8]> = self.sections.iter().map(Vec::as_slice).collect();
            self.body = write_sections(&[], &parts);
        }
        Ok(())
    }

    /// Whether the frame flagged `LAST` has been taken: the answer is whole.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.last
    }

    /// The frames taken as one Describe body once the frame flagged `LAST` is taken, its sections concatenated, read
    /// through [`DescribeView`] and checked by §8.3's Order; before it, no body.
    ///
    /// # Errors
    ///
    /// The first rule of §8.3's Order the concatenated body breaks; [`ProtocolError::body_too_short`] before the last
    /// frame.
    pub fn view(&self) -> Result<DescribeView<'_>, ProtocolError> {
        let header = self
            .command_header
            .as_ref()
            .unwrap_or(&[0; COMMAND_HEADER_SIZE]);
        DescribeView::read(CommandHeader(header), &self.body)
    }
}
