/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{StringList, as_u32};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::member::Member;
use crate::types::members::Members;
use crate::types::members_view::MembersView;
use crate::types::role_kind::RoleKind;
use crate::utils::body::{read_sections, read_string_list};
use crate::utils::command_header::expect_command;
use crate::utils::whole_records::whole_records;

impl<'a> BodyView<'a> for MembersView<'a> {
    const COMMAND: Command = Command::Members;

    type Body = Members;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[])?;
        let sections = read_sections::<3>(command, frame.body(), 0)?;
        let addresses = read_string_list(command, (1, 2), sections.get(1), sections.get(2))?;
        let members = sections.get(0);
        let start = sections.offset(0);
        let limit = as_u32(addresses.len());
        let mut previous: Option<Member> = None;
        for (i, bytes) in whole_records::<{ Member::SIZE }>(command, 0, members)?
            .iter()
            .enumerate()
        {
            let member = Member::decode(command, start + Member::SIZE * i, bytes)?;
            if member.address() >= limit {
                return Err(ProtocolError::index_out_of_range(
                    command,
                    0,
                    i,
                    member.address(),
                    limit,
                ));
            }
            if previous.is_some_and(|before| !in_order(&addresses, before, member)) {
                return Err(ProtocolError::field_value(
                    command,
                    "member order",
                    i as u64, // lossless: a `usize` is at most 64 bits
                ));
            }
            previous = Some(member);
        }
        Ok(Self { members, addresses })
    }

    fn to_body(&self) -> Result<Members, ProtocolError> {
        let mut members = Members::new();
        for (member, address) in
            (0..self.len()).filter_map(|i| Some((self.member(i)?, self.address(i)?)))
        {
            match member.kind() {
                RoleKind::Storage => members.push_storage(member.partition(), address),
                RoleKind::Compute | RoleKind::Frontend => members.push_compute(address),
            }
        }
        Ok(members)
    }
}

/// Whether `member` may follow `before`: compute before storage, compute by their addresses' bytes and storage by
/// partition, each ascending without repeat.
fn in_order(addresses: &StringList<'_>, before: Member, member: Member) -> bool {
    match (before.kind(), member.kind()) {
        (RoleKind::Compute, RoleKind::Compute) => {
            // Lossless: a `usize` is at least 32 bits.
            addresses.get(before.address() as usize) < addresses.get(member.address() as usize)
        }
        (RoleKind::Storage, RoleKind::Storage) => before.partition() < member.partition(),
        (RoleKind::Compute, RoleKind::Storage) => true,
        (RoleKind::Storage | RoleKind::Frontend, RoleKind::Compute)
        | (RoleKind::Frontend, RoleKind::Storage)
        | (RoleKind::Compute | RoleKind::Storage | RoleKind::Frontend, RoleKind::Frontend) => false,
    }
}
