/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod config_store_write;

use ultragraphdb_common_config_check::{Defect, TargetFiles, check_target};
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::errors::config_error::ConfigError;
use crate::types::config_record::ConfigRecord;
use crate::types::config_state::ConfigState;
use crate::types::target_state::TargetState;
use crate::types::target_summary::TargetSummary;

/// The bytes of rounds one read of the log returns at most, 1 MiB: a round at its largest.
const READ_BYTES: u32 = 1 << 20;

/// The cluster identity of the service's own log: it holds several clusters' settings and belongs to none.
pub const CONFIG_LOG_CLUSTER: u64 = 0x4749_464E_4F43;

/// The settings of every target over a log: the log is the store, and the state is what replaying it comes to.
#[derive(Debug)]
pub struct ConfigStore<L> {
    pub(crate) log: L,
    pub(crate) state: ConfigState,
}

impl<L: OrderedLog> ConfigStore<L> {
    /// The store over `log`, its state rebuilt by replaying every durable round.
    ///
    /// # Errors
    ///
    /// [`ConfigError::malformed_record`] or [`ConfigError::replay_diverged`] for a log this service did not write;
    /// the log's error when it cannot be read.
    pub async fn recover(mut log: L) -> Result<Self, ConfigError> {
        let mut state = ConfigState::default();
        let committed = log.committed();
        let mut last = Position::ZERO;
        while last < committed {
            let rounds = log.read(Position::new(last.get() + 1), READ_BYTES).await?;
            if rounds.is_empty() {
                return Err(ConfigError::replay_diverged(
                    last.get() + 1,
                    "a durable round",
                ));
            }
            for round in &rounds {
                state.apply(round.position().get(), ConfigRecord::decode(round.body())?)?;
                last = round.position();
            }
        }
        Ok(Self { log, state })
    }

    /// Every target, in name order.
    #[must_use]
    pub fn targets(&self) -> Vec<TargetSummary> {
        self.state
            .targets
            .iter()
            .map(|(name, state)| TargetSummary {
                name: name.clone(),
                latest: state.latest(),
                active: state.active,
                retired: state.retired,
            })
            .collect()
    }

    fn target(&self, target: &str) -> Result<&TargetState, ConfigError> {
        self.state
            .targets
            .get(target)
            .ok_or_else(|| ConfigError::unknown_target(target))
    }

    /// The merged settings of `instance` at `revision`, the active one when it is `None`, with the revision's number.
    ///
    /// # Errors
    ///
    /// Each reason a role cannot be served: the target unknown or retired, no active revision, the revision or the
    /// instance unknown.
    pub fn instance(
        &self,
        target: &str,
        revision: Option<u64>,
        instance: &str,
    ) -> Result<(u64, &str), ConfigError> {
        let state = self.target(target)?;
        if state.retired {
            return Err(ConfigError::retired(target));
        }
        let number = revision
            .or(state.active)
            .ok_or_else(|| ConfigError::no_active_revision(target))?;
        let held = state
            .revision(number)
            .ok_or_else(|| ConfigError::unknown_revision(target, number))?;
        let rendered = held
            .instances
            .get(instance)
            .ok_or_else(|| ConfigError::unknown_instance(target, number, instance))?;
        Ok((number, rendered))
    }

    /// Revision `revision`'s files, the bytes it was received as.
    ///
    /// # Errors
    ///
    /// The target or the revision unknown.
    pub fn files(&self, target: &str, revision: u64) -> Result<&[u8], ConfigError> {
        self.target(target)?
            .revision(revision)
            .map(|held| held.files.as_slice())
            .ok_or_else(|| ConfigError::unknown_revision(target, revision))
    }

    /// The active revision's files of `target`, read back.
    pub(crate) fn active_files(&self, target: &str) -> Option<TargetFiles> {
        let state = self.state.targets.get(target)?;
        let held = state.revision(state.active?)?;
        TargetFiles::from_bundle(target, &held.files).ok()
    }

    /// Every defect of every target's active revision by this release's checks, each naming its revision. A retired target
    /// serves nothing, so it stops nothing.
    #[must_use]
    pub fn active_defects(&self) -> Vec<Defect> {
        self.state
            .targets
            .iter()
            .filter(|(_, state)| !state.retired)
            .flat_map(|(name, state)| {
                let Some((number, held)) = state
                    .active
                    .and_then(|number| state.revision(number).map(|held| (number, held)))
                else {
                    return Vec::new();
                };
                let defects = match TargetFiles::from_bundle(name, &held.files) {
                    Ok(files) => check_target(&files, None),
                    Err(defect) => vec![*defect],
                };
                defects
                    .into_iter()
                    .map(|defect| defect.of_revision(number))
                    .collect()
            })
            .collect()
    }
}
