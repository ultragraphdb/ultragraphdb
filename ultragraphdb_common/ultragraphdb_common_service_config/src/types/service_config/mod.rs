/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::admin_table::AdminTable;
use crate::types::cluster_table::ClusterTable;
use crate::types::compute_table::ComputeTable;
use crate::types::config_table::ConfigTable;
use crate::types::endpoint_table::EndpointTable;
use crate::types::frontend_table::FrontendTable;
use crate::types::metadata_table::MetadataTable;
use crate::types::observability_table::ObservabilityTable;
use crate::types::runtime_table::RuntimeTable;
use crate::types::sequencer_table::SequencerTable;
use crate::types::settings_reader::SettingsReader;
use crate::types::storage_table::StorageTable;

/// The settings of one role's start: one type per table of the settings file, one field per setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceConfig {
    admin: AdminTable,
    cluster: ClusterTable,
    compute: ComputeTable,
    config: ConfigTable,
    endpoint: EndpointTable,
    frontend: FrontendTable,
    metadata: MetadataTable,
    observability: ObservabilityTable,
    runtime: RuntimeTable,
    sequencer: SequencerTable,
    storage: StorageTable,
}

impl ServiceConfig {
    /// The configuration `bytes` hold. Where they came from is the caller's: a role's come from the config service, and the
    /// config service's own from the file it is started with.
    ///
    /// # Errors
    ///
    /// The TOML refused; a table no type mirrors; a key no field takes; a value of the wrong kind, or outside its field.
    pub fn parse(bytes: &[u8]) -> Result<Self, ServiceConfigError> {
        let mut reader = SettingsReader::new(bytes)?;
        let config = Self::read(&mut reader)?;
        reader.finish()?;
        Ok(config)
    }

    /// Every refusal of a file holding `bytes`, not the first alone: an unknown table or key, or a value of the wrong
    /// kind. A document that does not parse is its one refusal.
    #[must_use]
    pub fn problems(bytes: &[u8]) -> Vec<ServiceConfigError> {
        let mut reader = match SettingsReader::collecting(bytes) {
            Ok(reader) => reader,
            Err(error) => return vec![error],
        };
        // A collecting reader records its refusals and refuses nothing.
        let _config = Self::read(&mut reader);
        reader.problems()
    }

    fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            admin: AdminTable::read(reader)?,
            cluster: ClusterTable::read(reader)?,
            compute: ComputeTable::read(reader)?,
            config: ConfigTable::read(reader)?,
            endpoint: EndpointTable::read(reader)?,
            frontend: FrontendTable::read(reader)?,
            metadata: MetadataTable::read(reader)?,
            observability: ObservabilityTable::read(reader)?,
            runtime: RuntimeTable::read(reader)?,
            sequencer: SequencerTable::read(reader)?,
            storage: StorageTable::read(reader)?,
        })
    }

    /// The `[admin]` table.
    #[must_use]
    pub fn admin(&self) -> &AdminTable {
        &self.admin
    }

    /// The `[config]` table.
    #[must_use]
    pub fn config(&self) -> &ConfigTable {
        &self.config
    }

    /// The `[cluster]` table.
    #[must_use]
    pub fn cluster(&self) -> &ClusterTable {
        &self.cluster
    }

    /// The `[compute]` table.
    #[must_use]
    pub fn compute(&self) -> &ComputeTable {
        &self.compute
    }

    /// The `[endpoint]` table.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointTable {
        &self.endpoint
    }

    /// The `[frontend]` table.
    #[must_use]
    pub fn frontend(&self) -> &FrontendTable {
        &self.frontend
    }

    /// The `[metadata]` table.
    #[must_use]
    pub fn metadata(&self) -> &MetadataTable {
        &self.metadata
    }

    /// The `[observability]` table.
    #[must_use]
    pub fn observability(&self) -> &ObservabilityTable {
        &self.observability
    }

    /// The `[runtime]` table.
    #[must_use]
    pub fn runtime(&self) -> &RuntimeTable {
        &self.runtime
    }

    /// The `[sequencer]` table.
    #[must_use]
    pub fn sequencer(&self) -> &SequencerTable {
        &self.sequencer
    }

    /// The `[storage]` table.
    #[must_use]
    pub fn storage(&self) -> &StorageTable {
        &self.storage
    }
}
