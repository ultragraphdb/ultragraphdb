/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A session token in a command header (`PROTOCOL.md` §4.8): 24 bytes at a command's offset, `partition` u32, 4 zero
//! bytes, `position` u64, `epoch` u64; all zero for no token.

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::command_header::CommandHeader;
use crate::utils::constants::COMMAND_HEADER_SIZE;

/// The token's width in a command header.
pub(crate) const TOKEN_SIZE: usize = 24;

/// The token's 24 bytes; zero for `None`.
pub(crate) fn encode(token: Option<&SessionToken>) -> [u8; TOKEN_SIZE] {
    let mut out = [0u8; TOKEN_SIZE];
    if let Some(token) = token {
        let fields = token
            .partition()
            .to_le_bytes()
            .into_iter()
            .chain([0; 4])
            .chain(token.position().to_le_bytes())
            .chain(token.epoch().get().to_le_bytes());
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
    }
    out
}

/// The token at `at` of `header`, checked in order: bytes 4 to 7 zero, refused as
/// [`ProtocolError::reserved_not_zero`] at their frame offset; then every field 0, `None`, or `partition ≥ 1`,
/// `position ≥ 1` and `epoch` 1 to 2⁴² − 1, refused as [`ProtocolError::field_value`] (`"token"`) with the first
/// field broken.
pub(crate) fn read(
    command: u16,
    header: CommandHeader<'_>,
    at: usize,
) -> Result<Option<SessionToken>, ProtocolError> {
    // `at` is a field's place within the 128-byte command header.
    let field = |offset: usize| {
        at.checked_add(offset)
            .ok_or_else(|| ProtocolError::field_value(command, "token", at as u64)) // lossless: a `usize` is at most 64 bits
    };
    for i in 4..8 {
        let place = field(i)?;
        if header.u8_at(place) != 0 {
            // A place within the command header, which the refusal only names.
            return Err(ProtocolError::reserved_not_zero(
                COMMAND_HEADER_SIZE.saturating_add(place),
            ));
        }
    }
    let partition = header.u32_at(at);
    let position = header.u64_at(field(8)?);
    let epoch = header.u64_at(field(16)?);
    if partition == 0 && position == 0 && epoch == 0 {
        return Ok(None);
    }
    if partition == 0 {
        return Err(ProtocolError::field_value(command, "token", 0));
    }
    if position == 0 {
        return Err(ProtocolError::field_value(command, "token", 0));
    }
    match Epoch::new(epoch) {
        Ok(e) if epoch != 0 => Ok(Some(SessionToken::new(partition, position, e))),
        Ok(_) | Err(_) => Err(ProtocolError::field_value(command, "token", epoch)),
    }
}

/// The token at `at` of a Commit's or a Drop's `header`, read as [`read`] reads it, then held to its reply rule
/// (`PROTOCOL.md` §6.3, §6.5): none in a request, `commit_version` 0; in a reply through `session`, a token of epoch
/// `commit_version >> 22`; in a reply through no session, `session` all zero, none. Refused as
/// [`ProtocolError::field_value`] (`"token"`) with the token's epoch, 0 for none.
pub(crate) fn commit_token(
    command: u16,
    header: CommandHeader<'_>,
    at: usize,
    commit_version: u64,
    session: [u8; 16],
) -> Result<Option<SessionToken>, ProtocolError> {
    let token = read(command, header, at)?;
    let epoch = token.as_ref().map_or(0, |t| t.epoch().get());
    let expects_token = commit_version != 0 && session != [0; 16];
    let expected = if expects_token {
        commit_version >> 22
    } else {
        0
    };
    if epoch != expected || (expects_token && token.is_none()) {
        return Err(ProtocolError::field_value(command, "token", epoch));
    }
    Ok(token)
}
