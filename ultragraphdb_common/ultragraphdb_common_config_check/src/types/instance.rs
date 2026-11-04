/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::ServiceConfig;

/// One instance of a target: its name, its role, and its settings merged and rendered canonically, so two reads of one
/// revision are byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub(crate) name: String,
    pub(crate) role: &'static str,
    pub(crate) rendered: String,
    pub(crate) config: Option<ServiceConfig>,
}

impl Instance {
    /// The instance's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its role's name.
    #[must_use]
    pub fn role(&self) -> &'static str {
        self.role
    }

    /// Its merged settings, rendered.
    #[must_use]
    pub fn rendered(&self) -> &str {
        &self.rendered
    }

    /// Its settings, when they parse.
    #[must_use]
    pub fn config(&self) -> Option<&ServiceConfig> {
        self.config.as_ref()
    }
}
