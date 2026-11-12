/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::closed::Closed;
use crate::types::closed_partition::ClosedPartition;
use crate::types::closed_view::{ClosedView, PARTITION_SIZE};
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::body::{read_sections, records};
use crate::utils::command_header::expect_command;
use crate::utils::fields::epoch;

impl<'a> BodyView<'a> for ClosedView<'a> {
    const COMMAND: Command = Command::Closed;

    type Body = Closed;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[])?;
        let sections = read_sections::<1>(command, frame.body(), 0)?;
        let partitions = sections.get(0);
        let whole = records(command, 0, partitions, PARTITION_SIZE)?;
        if partitions.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "partitions", 0, 1));
        }
        let mut previous = 0u32;
        for (index, record) in whole.enumerate() {
            if let Some(at) = record
                .get(4..8)
                .and_then(|zero| zero.iter().position(|b| *b != 0))
            {
                // A place within one frame, which a refusal names: far below `usize::MAX`, so it never saturates.
                let base = sections
                    .offset(0)
                    .saturating_add(PARTITION_SIZE.saturating_mul(index));
                return Err(ProtocolError::padding_not_zero(
                    command,
                    base.saturating_add(4).saturating_add(at),
                ));
            }
            let bytes: [u8; PARTITION_SIZE] = record.try_into().unwrap_or([0; PARTITION_SIZE]);
            let partition = ClosedPartition::decode(&bytes);
            if partition.partition() <= previous {
                return Err(ProtocolError::field_value(
                    command,
                    "partition",
                    partition.partition().into(),
                ));
            }
            let _epoch: u64 = epoch(command, "epoch", partition.epoch(), true)?;
            previous = partition.partition();
        }
        Ok(Self { partitions })
    }

    fn to_body(&self) -> Result<Closed, ProtocolError> {
        Ok(Closed::new(self.partitions().collect()))
    }
}
