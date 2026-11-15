/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;

/// A command's body read in place: constructed from a validated frame, its command header and every section checked
/// once, every value afterwards read through the view, which borrows the receive buffer.
pub trait BodyView<'a>: Sized {
    /// The command whose body this is.
    const COMMAND: Command;

    /// The owned body the view reads back to.
    type Body: BodyBuilder;

    /// The body of `frame`, checked: the frame's command is [`BodyView::COMMAND`], refused as
    /// [`ProtocolError::wrong_command`]; then the command header's fields and unused bytes; then the body's section
    /// table, sections and their contents.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::wrong_command`] for a frame of another command, else the refusal of the first rule the
    /// body breaks.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError>;

    /// The owned body holding the view's values, which builds the same frame again.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::bad_term`] where a value is parsed rather than read in place: an RDF term or an IRI not well
    /// formed.
    fn to_body(&self) -> Result<Self::Body, ProtocolError>;
}
