/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlError;

use crate::errors::service_config_error::{ServiceConfigError, ServiceConfigErrorEnum};

impl From<TomlError> for ServiceConfigError {
    fn from(error: TomlError) -> Self {
        Self(ServiceConfigErrorEnum::Toml(error))
    }
}
