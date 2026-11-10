/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::verify::Verify;
use crate::types::verify_view::VerifyView;
use crate::utils::body::{read_sections, text};
use crate::utils::command_header::expect_command;
use crate::utils::fields::boolean;

/// The size of a ClientProof and of a ServerSignature: an HMAC-SHA-256.
const PROOF_SIZE: usize = 32;

/// `bytes` as a 32-byte proof or signature, refused as [`ProtocolError::count_mismatch`] naming `field` otherwise.
fn proof<'a>(
    command: u16,
    field: &'static str,
    bytes: &'a [u8],
) -> Result<&'a [u8; PROOF_SIZE], ProtocolError> {
    bytes.try_into().map_err(|_| {
        // Lossless: a `usize` is at most 64 bits.
        ProtocolError::count_mismatch(command, field, bytes.len() as u64, PROOF_SIZE as u64)
    })
}

impl<'a> BodyView<'a> for VerifyView<'a> {
    const COMMAND: Command = Command::Verify;

    type Body = Verify;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 16)])?;
        let verified = boolean(command, "verified", header.u8_at(0))?;
        let principal = header.u64_at(8);
        if !verified && principal != 0 {
            return Err(ProtocolError::field_value(command, "principal", principal));
        }
        let sections = read_sections::<4>(command, frame.body(), 0)?;
        let name = text(command, 0, sections.get(0))?;
        let auth_message = text(command, 1, sections.get(1))?;
        let client_proof = proof(command, "proof", sections.get(2))?;
        let signed = sections.get(3);
        let signature = if verified {
            Some(proof(command, "signature", signed)?)
        } else if signed.is_empty() {
            None
        } else {
            return Err(ProtocolError::count_mismatch(
                command,
                "signature",
                signed.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        };
        Ok(Self {
            name,
            auth_message,
            proof: client_proof,
            verified,
            principal,
            signature,
        })
    }

    fn to_body(&self) -> Result<Verify, ProtocolError> {
        let request = Verify::request(self.name, self.auth_message, *self.proof);
        Ok(match self.signature {
            Some(signature) => request.with_verified(self.principal, *signature),
            None => request,
        })
    }
}
