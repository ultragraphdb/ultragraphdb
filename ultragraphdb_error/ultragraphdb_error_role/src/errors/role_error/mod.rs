/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod role_error_display;
mod role_error_error;

/// Why a role stopped. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleErrorEnum {
    /// An unknown, malformed, missing or out-of-bounds setting, or an unreadable secret or certificate file.
    Configuration {
        /// What was refused, naming the setting.
        message: String,
    },
    /// What the role needs is not available, such as an address it cannot listen at.
    Unavailable {
        /// What is unavailable, and why.
        message: String,
    },
}

/// A role's reason to stop, carrying its exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleError(pub RoleErrorEnum);

impl RoleError {
    /// `EX_UNAVAILABLE`: a dependency lost, or a resource the role needs refused.
    pub const EX_UNAVAILABLE: u8 = 69;
    /// `EX_CONFIG`: a configuration error.
    pub const EX_CONFIG: u8 = 78;

    /// A refused configuration, `message` naming the setting.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self(RoleErrorEnum::Configuration {
            message: message.into(),
        })
    }

    /// Something the role needs is unavailable.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self(RoleErrorEnum::Unavailable {
            message: message.into(),
        })
    }

    /// The code the process exits with.
    pub fn exit_code(&self) -> u8 {
        match self.0 {
            RoleErrorEnum::Configuration { .. } => Self::EX_CONFIG,
            RoleErrorEnum::Unavailable { .. } => Self::EX_UNAVAILABLE,
        }
    }
}
