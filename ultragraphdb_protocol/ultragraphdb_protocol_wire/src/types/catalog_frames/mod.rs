/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::catalog_view::CatalogView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::constants::{CATALOG_BYTES_MAX, COMMAND_HEADER_SIZE, FLAG_LAST};

/// A Catalog reply's frames read as one version (`PROTOCOL.md` §13.7): a version's bytes, at most
/// [`crate::CATALOG_BYTES_MAX`], that do not fit one frame within the requester's `max_frame` continue in further
/// Catalog frames on the stream, each with the first's command header, the last alone flagged `LAST`; the requester
/// reads section 0 as the concatenation of every frame's, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogFrames {
    command_header: Option<[u8; COMMAND_HEADER_SIZE]>,
    bytes: Vec<u8>,
    last: bool,
    frames: u64,
}

impl CatalogFrames {
    /// No frame taken yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes `frame`, the reply's next frame, read through [`CatalogView`] and checked against the frames before: no
    /// frame taken flagged `LAST`, and its command header the first frame's ([`ProtocolError::field_value`]
    /// `"catalog"`, naming the frames taken); the version's bytes, this frame's with them, at most
    /// [`CATALOG_BYTES_MAX`] (`"catalog"`, naming their bytes), so assembling a version is bounded.
    ///
    /// # Errors
    ///
    /// The refusal of the frame's view, or a command header other than the first frame's.
    pub fn push(&mut self, frame: &FrameView<'_>) -> Result<(), ProtocolError> {
        let view = CatalogView::from_frame(frame)?;
        let header = frame.header().command_header();
        let other = self.command_header.is_some_and(|first| first != *header);
        if self.last || other {
            return Err(ProtocolError::field_value(
                Command::Catalog.number(),
                "catalog",
                self.frames,
            ));
        }
        let held = self.bytes.len().saturating_add(view.bytes().len());
        if held > CATALOG_BYTES_MAX {
            return Err(ProtocolError::field_value(
                Command::Catalog.number(),
                "catalog",
                held as u64, // lossless: a `usize` is at most 64 bits
            ));
        }
        self.bytes.extend_from_slice(view.bytes());
        self.command_header = Some(*header);
        self.frames = self.frames.saturating_add(1);
        self.last = frame.header().flags() & FLAG_LAST != 0;
        Ok(())
    }

    /// Whether the frame flagged `LAST` has been taken: the version is whole.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.last
    }

    /// The reply as one frame would carry it: the first frame's fields and every frame's section 0 concatenated;
    /// `None` before the first frame.
    #[must_use]
    pub fn view(&self) -> Option<CatalogView<'_>> {
        let header = self.command_header.as_ref()?;
        Some(CatalogView::assembled(header, &self.bytes))
    }
}
