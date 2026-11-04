/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::role_error::{RoleError, RoleErrorEnum};

impl fmt::Display for RoleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            RoleErrorEnum::Configuration { message } => write!(f, "configuration: {message}"),
            RoleErrorEnum::Unavailable { message } => write!(f, "unavailable: {message}"),
            RoleErrorEnum::Io { message } => write!(f, "io: {message}"),
            RoleErrorEnum::Software { message } => write!(f, "software: {message}"),
        }
    }
}
