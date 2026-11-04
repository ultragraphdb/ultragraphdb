/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::{TargetFiles, check_target};
use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::{OrderedLog, RoundRef};

use crate::errors::config_error::ConfigError;
use crate::types::config_record::ConfigRecord;
use crate::types::config_store::ConfigStore;

/// The transaction count and derivation a round of this log carries, which only the commit path reads.
const DERIVATION: std::num::NonZeroU32 = std::num::NonZeroU32::MIN;

impl<L: OrderedLog> ConfigStore<L> {
    /// Adds `body`'s files as `target`'s next revision, edited from `base`, 0 beginning a target; the active revision stays
    /// as it was. The number it took is returned once it is durable.
    ///
    /// # Errors
    ///
    /// [`ConfigError::stale`] when `base` is not the latest; [`ConfigError::refused`] with every defect, nothing stored;
    /// the log's error.
    pub async fn add_revision(
        &mut self,
        target: &str,
        base: u64,
        body: &[u8],
    ) -> Result<u64, ConfigError> {
        let latest = self
            .state
            .targets
            .get(target)
            .map_or(0, |state| state.latest());
        if base != latest {
            return Err(ConfigError::stale(target, base, latest));
        }
        let files = TargetFiles::from_bundle(target, body)
            .map_err(|defect| ConfigError::refused(vec![*defect]))?;
        let defects = check_target(&files, self.active_files(target).as_ref());
        if !defects.is_empty() {
            return Err(ConfigError::refused(defects));
        }
        let number = latest + 1;
        self.commit(ConfigRecord::Revision {
            target,
            number,
            activate: false,
            files: body,
        })
        .await?;
        Ok(number)
    }

    /// Makes `revision` active, older or newer, which ends a retirement.
    ///
    /// # Errors
    ///
    /// The target or revision unknown; [`ConfigError::refused`] when the revision changes what the active one fixes; the
    /// log's error.
    pub async fn activate(&mut self, target: &str, revision: u64) -> Result<(), ConfigError> {
        let files = TargetFiles::from_bundle(target, self.files(target, revision)?)
            .map_err(|defect| ConfigError::refused(vec![*defect]))?;
        let defects = check_target(&files, self.active_files(target).as_ref());
        if !defects.is_empty() {
            return Err(ConfigError::refused(defects));
        }
        self.commit(ConfigRecord::Activate {
            target,
            number: revision,
        })
        .await
    }

    /// Retires `target`: it serves no instance, and its revisions stay stored.
    ///
    /// # Errors
    ///
    /// The target unknown; the log's error.
    pub async fn retire(&mut self, target: &str) -> Result<(), ConfigError> {
        if !self.state.targets.contains_key(target) {
            return Err(ConfigError::unknown_target(target));
        }
        self.commit(ConfigRecord::Retire { target }).await
    }

    /// Begins each of `declared` holding no revision, its files revision 1 and active in one durable step. A target holding
    /// a revision stays as it is, since the API is the authority over it: each whose active files differ from the declared
    /// is returned with its active revision, for the log.
    ///
    /// # Errors
    ///
    /// The log's error.
    pub async fn seed(
        &mut self,
        declared: &[TargetFiles],
    ) -> Result<Vec<(String, Option<u64>)>, ConfigError> {
        let mut differing = Vec::new();
        for files in declared {
            let name = files.name();
            match self.state.targets.get(name) {
                None => {
                    self.commit(ConfigRecord::Revision {
                        target: name,
                        number: 1,
                        activate: true,
                        files: &files.to_bundle(),
                    })
                    .await?;
                }
                Some(state) => {
                    let active = state.active;
                    let same = self
                        .active_files(name)
                        .is_some_and(|held| held.same_files(files));
                    if !same {
                        differing.push((name.to_string(), active));
                    }
                }
            }
        }
        Ok(differing)
    }

    /// Appends `record`, waits until it is durable, and only then applies it.
    async fn commit(&mut self, record: ConfigRecord<'_>) -> Result<(), ConfigError> {
        let (tail, parent) = self.log.tail();
        let position = tail.next().ok_or_else(|| LogError::exhausted(tail.get()))?;
        let body = record.encode();
        let _ticket = self
            .log
            .append(RoundRef::new(position, parent, 0, 1, DERIVATION, &body))?;
        let _committed = self.log.sync().await?;
        self.state.apply(position.get(), record)
    }
}
