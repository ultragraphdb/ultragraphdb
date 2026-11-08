/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::join::Join;
use crate::types::join_view::JoinView;
use crate::types::role_kind::RoleKind;
use crate::utils::sections::{Sections, expect_command, text};
use crate::utils::whole_records::whole_records;

impl<'a> BodyView<'a> for JoinView<'a> {
    const COMMAND: Command = Command::Join;

    type Body = Join;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1)])?;
        let number = header.u8_at(0);
        let kind = match RoleKind::from_number(number) {
            Some(kind @ (RoleKind::Compute | RoleKind::Storage)) => kind,
            Some(RoleKind::Frontend) | None => {
                return Err(ProtocolError::field_value(command, "kind", number.into()));
            }
        };
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let address = text(command, 0, sections.get(0))?;
        if !address.starts_with("tcp://") && !address.starts_with("unix://") {
            return Err(ProtocolError::field_value(
                command,
                "address",
                address.len() as u64, // lossless: a `usize` is at most 64 bits
            ));
        }
        let partitions = sections.get(1);
        let mut previous = 0u32;
        for partition in whole_records::<4>(command, 1, partitions)? {
            let partition = u32::from_le_bytes(*partition);
            if partition <= previous {
                return Err(ProtocolError::field_value(
                    command,
                    "partition",
                    partition.into(),
                ));
            }
            previous = partition;
        }
        let count = (partitions.len() / 4) as u64; // lossless: a `usize` is at most 64 bits
        let expected = u64::from(kind == RoleKind::Storage);
        if (count == 0) != (expected == 0) {
            return Err(ProtocolError::count_mismatch(
                command,
                "partitions",
                count,
                expected,
            ));
        }
        Ok(Self {
            kind,
            address,
            partitions,
        })
    }

    fn to_body(&self) -> Result<Join, ProtocolError> {
        let partitions = (0..self.partition_count())
            .filter_map(|i| self.partition(i))
            .collect();
        Ok(match self.kind {
            RoleKind::Storage => Join::storage(self.address, partitions),
            RoleKind::Compute | RoleKind::Frontend => Join::compute(self.address),
        })
    }
}
