/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frame built only of a body its decoder reads (`BodyBuilder`'s rule): the builder lays the body out, the body's
//! view reads the bytes back through the checks it gives a received frame, and its refusal is the builder's.

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::header::Header;
use crate::utils::command_header::CommandHeader;
use crate::utils::constants::COMMAND_HEADER_SIZE;

/// The frame of `command` under `header` holding `command_header` and `body`, once `read`, the view's reader of a
/// command header and a body, takes them.
///
/// # Errors
///
/// The refusal `read` gives, and no frame; [`ProtocolError::frame_too_large`] for a frame past 1 MiB.
pub(crate) fn checked_frame<'b, V>(
    command: Command,
    header: &Header,
    command_header: &'b [u8; COMMAND_HEADER_SIZE],
    body: &'b [u8],
    read: impl FnOnce(CommandHeader<'b>, &'b [u8]) -> Result<V, ProtocolError>,
) -> Result<Vec<u8>, ProtocolError> {
    let _read: V = read(CommandHeader(command_header), body)?;
    header
        .clone()
        .with_command(command)
        .with_command_header(*command_header)
        .encode_frame(body)
}
