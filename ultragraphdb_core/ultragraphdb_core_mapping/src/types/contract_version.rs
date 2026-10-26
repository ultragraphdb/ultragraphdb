/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::mapping_error::MappingError;

/// The version of the mapping contract a store is written under. A build opens a store of its own major version and
/// refuses any other; a minor version adds what an older build of the same major version can still read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContractVersion {
    major: u16,
    minor: u16,
}

/// The version this build implements.
pub const CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 0);

impl ContractVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub fn major(&self) -> u16 {
        self.major
    }

    pub fn minor(&self) -> u16 {
        self.minor
    }

    /// The four bytes a store records it under: the major then the minor version, each a u16 big-endian.
    pub fn to_bytes(&self) -> [u8; 4] {
        let [a, b] = self.major.to_be_bytes();
        let [c, d] = self.minor.to_be_bytes();
        [a, b, c, d]
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MappingError> {
        match bytes {
            [a, b, c, d] => Ok(Self::new(
                u16::from_be_bytes([*a, *b]),
                u16::from_be_bytes([*c, *d]),
            )),
            _ => Err(MappingError::BadContractVersion(bytes)),
        }
    }

    /// Refuses a store recorded under `stored` when its major version is not this one's.
    pub fn check_stored(&self, stored: ContractVersion) -> Result<(), MappingError> {
        if stored.major == self.major {
            Ok(())
        } else {
            Err(MappingError::ContractVersion(
                (stored.major, stored.minor),
                (self.major, self.minor),
            ))
        }
    }
}
