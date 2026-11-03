/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A catalog announcement of an EpochClose (`PROTOCOL.md` §13.4), 16 bytes: `namespace` u64, then `version` u64, the
/// catalog version taking effect at the epoch one above the close's, at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogAnnouncement {
    namespace: u64,
    version: u64,
}

impl CatalogAnnouncement {
    /// An announcement's size in bytes.
    pub const SIZE: usize = 16;

    /// The announcement of `namespace`'s catalog `version`.
    pub fn new(namespace: u64, version: u64) -> Self {
        Self { namespace, version }
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The catalog version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The announcement's 16 bytes.
    pub fn encode(&self) -> [u8; CatalogAnnouncement::SIZE] {
        let mut out = [0u8; CatalogAnnouncement::SIZE];
        out.iter_mut()
            .zip(
                self.namespace
                    .to_le_bytes()
                    .into_iter()
                    .chain(self.version.to_le_bytes()),
            )
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The announcement `bytes` hold.
    pub(crate) fn decode(bytes: &[u8; CatalogAnnouncement::SIZE]) -> Self {
        let (namespace, version) = bytes.split_at(8);
        Self::new(
            namespace.try_into().map_or(0, u64::from_le_bytes),
            version.try_into().map_or(0, u64::from_le_bytes),
        )
    }
}
