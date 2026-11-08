/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[cluster]` table: what every role of a cluster shares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClusterTable {
    id: Option<u64>,
    role_secret_file: Option<String>,
    frontend_address: Option<String>,
    partition_count: Option<u32>,
    join_deadline_ms: Option<u64>,
}

impl ClusterTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            id: reader.u64("cluster", "id")?,
            role_secret_file: reader.string("cluster", "role_secret_file")?,
            frontend_address: reader.string("cluster", "frontend_address")?,
            partition_count: reader.u32("cluster", "partition_count")?,
            join_deadline_ms: reader.u64("cluster", "join_deadline_ms")?,
        })
    }

    /// `id`: the cluster's identity, which every frame's header names.
    #[must_use]
    pub fn id(&self) -> Option<u64> {
        self.id
    }

    /// `role_secret_file`: the file holding the secret roles authenticate each other with.
    #[must_use]
    pub fn role_secret_file(&self) -> Option<&str> {
        self.role_secret_file.as_deref()
    }

    /// `frontend_address`: the frontend's role address, which storage and compute join at.
    #[must_use]
    pub fn frontend_address(&self) -> Option<&str> {
        self.frontend_address.as_deref()
    }

    /// `partition_count`: the partitions the frontend forms the placement over.
    #[must_use]
    pub fn partition_count(&self) -> Option<u32> {
        self.partition_count
    }

    /// `join_deadline_ms`: how long storage and compute retry joining, and the frontend connecting back.
    #[must_use]
    pub fn join_deadline_ms(&self) -> Option<u64> {
        self.join_deadline_ms
    }
}
