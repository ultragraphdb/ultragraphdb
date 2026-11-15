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
///
/// A body's builder refuses to build what its decoder refuses: [`BodyBuilder::frame`] gives the refusal the body's
/// view would give the same bytes, and no frame. This is the rule every new body follows, and the builders of
/// `Session`, `Check`, `Ended`, `Declare`, `Defaults`, `Challenge` and `Verify` hold it in full. `ErrorReply`'s holds
/// it for its `detail`: a `WrongPartition` reply whose `detail` has any of bits 24 to 31 set, or an `OperationFailed`
/// reply whose `detail` has any bit but bit 0 set, is refused as [`ProtocolError::field_value`] (`"detail"`), the
/// rules its view holds (`PROTOCOL.md` §9.4).
///
/// The builders of the other bodies are permissive: each writes the fields and terms it is given as they are, so a
/// sender's mistake reaches the wire and the receiving command's view refuses it, and their [`BodyBuilder::frame`]
/// refuses only a frame past 1 MiB.
pub trait BodyBuilder {
    /// The command whose body this is.
    const COMMAND: Command;

    /// The command header: the body's fixed fields, every unused byte zero.
    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE];

    /// The body before its chunk table: the section table and the sections.
    fn body(&self) -> Vec<u8>;

    /// The frame of this body under `header`, whose command and command header are replaced by the body's.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::frame_too_large`] for a frame past 1 MiB; and, for a body whose builder holds the rule above,
    /// the refusal of the first rule the body breaks.
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        header
            .with_command(Self::COMMAND)
            .with_command_header(self.command_header())
            .encode_frame(&self.body())
    }
}
