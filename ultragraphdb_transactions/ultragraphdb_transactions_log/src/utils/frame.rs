/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, HEADER_SIZE, Header, RoundHeader};
use ultragraphdb_traits_log::RoundRef;

use crate::types::aligned_buf::AlignedBuf;
use crate::utils::block::padded;

/// The bytes a round takes in a segment and its checksum: the Round frame of `cluster`, its header numbered by the
/// round's position and naming `release`, the release of the node writing it (`PROTOCOL.md` §12), its command header
/// the round's position, parent, `incarnation`, first version and transaction count, its body the round's body,
/// padded with zeros to whole 4,096-byte blocks. The checksum is the frame's header `checksum`, the round's identity.
/// Refused as [`LogError::TooLarge`] when the frame would exceed the 1 MiB maximum.
pub fn encode_round(
    cluster: u64,
    release: u32,
    incarnation: u64,
    round: &RoundRef<'_>,
) -> Result<(AlignedBuf, u64), LogError> {
    let command_header = RoundHeader::new(
        round.position().get(),
        round.parent(),
        incarnation,
        round.first_version(),
        round.transactions(),
    )
    .encode();
    let frame = Header::new(cluster, Command::Round)
        .with_sequence(u128::from(round.position().get()))
        .with_release(release)
        .with_command_header(command_header)
        .encode_frame(round.body())
        .map_err(|refusal| match refusal.0 {
            ProtocolErrorEnum::FrameTooLarge { size, limit } => LogError::TooLarge(size, limit),
            // encode_frame refuses a frame for its size alone.
            _ => LogError::TooLarge(HEADER_SIZE + round.body().len(), 0),
        })?;
    let mut bytes = AlignedBuf::zeroed(padded(frame.len() as u64) as usize);
    bytes
        .as_mut_slice()
        .iter_mut()
        .zip(&frame)
        .for_each(|(out, byte)| *out = *byte);
    let checksum = frame
        .first_chunk::<8>()
        .map_or(0, |bytes| u64::from_le_bytes(*bytes));
    Ok((bytes, checksum))
}
