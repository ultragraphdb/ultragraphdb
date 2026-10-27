/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::header::Header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

/// A command's body as a sender holds it: its command header's 128 bytes and its body's bytes, laid out as the
/// command's view reads them.
pub trait BodyBuilder {
    /// The command whose body this is.
    const COMMAND: Command;

    /// The command header: the body's fixed fields, every unused byte zero.
    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE];

    /// The body before its chunk table: the section table and the sections.
    fn body(&self) -> Vec<u8>;

    /// The frame of this body under `header`, whose command and command header are replaced by the body's; refused as
    /// [`ProtocolError::FrameTooLarge`] past 1 MiB.
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        header
            .with_command(Self::COMMAND)
            .with_command_header(self.command_header())
            .encode_frame(&self.body())
    }
}
