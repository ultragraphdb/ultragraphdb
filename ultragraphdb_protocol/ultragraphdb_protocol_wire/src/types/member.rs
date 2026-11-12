/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::role_kind::RoleKind;

/// A member of a Members reply (`PROTOCOL.md` §13.2), 12 bytes: `kind` u8, 2 compute or 3 storage; 3 zero bytes;
/// `partition` u32, for storage a partition it hosts, 0 for compute; `address` u32, the role address's index in the
/// body's string list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Member {
    kind: RoleKind,
    partition: u32,
    address: u32,
}

impl Member {
    /// A member's size in bytes.
    pub const SIZE: usize = 12;

    /// The member of `kind` hosting `partition` (0 for compute) at the string `address`.
    #[must_use]
    pub fn new(kind: RoleKind, partition: u32, address: u32) -> Self {
        Self {
            kind,
            partition,
            address,
        }
    }

    /// The member's kind.
    #[must_use]
    pub fn kind(&self) -> RoleKind {
        self.kind
    }

    /// The partition a storage member hosts; 0 for compute.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The role address's index in the string list.
    #[must_use]
    pub fn address(&self) -> u32 {
        self.address
    }

    /// The member's 12 bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; Member::SIZE] {
        let fields = [self.kind.number(), 0, 0, 0]
            .into_iter()
            .chain(self.partition.to_le_bytes())
            .chain(self.address.to_le_bytes());
        let mut out = [0u8; Member::SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The member `bytes` hold, at body offset `at` of a body of `command`, checked in order: `kind` 2 or 3
    /// ([`ProtocolError::field_value`] `"kind"`), bytes 1 to 3 zero ([`ProtocolError::padding_not_zero`]), `partition`
    /// 0 for compute and at least 1 for storage ([`ProtocolError::field_value`] `"partition"`).
    pub(crate) fn decode(
        command: u16,
        at: usize,
        bytes: &[u8; Member::SIZE],
    ) -> Result<Self, ProtocolError> {
        let [number, pad1, pad2, pad3, p0, p1, p2, p3, a0, a1, a2, a3] = *bytes;
        let kind = match RoleKind::from_number(number) {
            Some(kind @ (RoleKind::Compute | RoleKind::Storage)) => kind,
            Some(RoleKind::Frontend) | None => {
                return Err(ProtocolError::field_value(command, "kind", number.into()));
            }
        };
        if let Some(i) = [pad1, pad2, pad3].iter().position(|b| *b != 0) {
            return Err(ProtocolError::padding_not_zero(
                command,
                at.saturating_add(1).saturating_add(i),
            ));
        }
        let partition = u32::from_le_bytes([p0, p1, p2, p3]);
        if (kind == RoleKind::Compute) != (partition == 0) {
            return Err(ProtocolError::field_value(
                command,
                "partition",
                partition.into(),
            ));
        }
        Ok(Self::new(
            kind,
            partition,
            u32::from_le_bytes([a0, a1, a2, a3]),
        ))
    }
}
