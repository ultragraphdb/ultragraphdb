/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::mapping_error::{MappingError, MappingErrorEnum};

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            MappingErrorEnum::ContractVersion {
                stored,
                implemented,
            } => write!(
                f,
                "The store was written under mapping contract version {}.{}; this build implements version {}.{}",
                stored.0, stored.1, implemented.0, implemented.1
            ),
            MappingErrorEnum::BadContractVersion { bytes } => {
                write!(f, "Unreadable mapping contract version {bytes:02x?}")
            }
            MappingErrorEnum::NoSuchStatement { statement } => {
                write!(f, "No statement {statement} is held")
            }
            MappingErrorEnum::AmbiguousStatement { statement } => write!(
                f,
                "{statement} is held by several statements; name one by its statement ID"
            ),
            MappingErrorEnum::UnsupportedObject { object } => {
                write!(
                    f,
                    "The object {object} is a triple term, which no write stores"
                )
            }
        }
    }
}
