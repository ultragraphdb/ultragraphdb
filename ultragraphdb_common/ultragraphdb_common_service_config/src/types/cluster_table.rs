/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[cluster]` table: what every role of a cluster shares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClusterTable {
    role_secret_file: Option<String>,
    frontend_address: Option<String>,
    partition_count: Option<u32>,
}

impl ClusterTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            role_secret_file: reader.string("cluster", "role_secret_file")?,
            frontend_address: reader.string("cluster", "frontend_address")?,
            partition_count: reader.u32("cluster", "partition_count")?,
        })
    }

    /// `role_secret_file`: the file holding the secret roles authenticate each other with.
    pub fn role_secret_file(&self) -> Option<&str> {
        self.role_secret_file.as_deref()
    }

    /// `frontend_address`: the frontend's role address, which storage and compute join at.
    pub fn frontend_address(&self) -> Option<&str> {
        self.frontend_address.as_deref()
    }

    /// `partition_count`: the partitions the frontend forms the placement over.
    pub fn partition_count(&self) -> Option<u32> {
        self.partition_count
    }
}
