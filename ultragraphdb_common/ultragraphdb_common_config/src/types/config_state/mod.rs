/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::errors::config_error::ConfigError;
use crate::types::config_record::ConfigRecord;
use crate::types::revision::Revision;
use crate::types::target_state::TargetState;

/// What the log's records come to: every target's revisions and which is active.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfigState {
    pub(crate) targets: BTreeMap<String, TargetState>,
}

impl ConfigState {
    /// Applies `record`, the log's round at `position`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::replay_diverged`] when it does not follow from the state, which only a log this service did not
    /// write can hold.
    pub(crate) fn apply(
        &mut self,
        position: u64,
        record: ConfigRecord<'_>,
    ) -> Result<(), ConfigError> {
        match record {
            ConfigRecord::Revision {
                target,
                number,
                activate,
                files,
            } => {
                let state = self.targets.entry(target.to_string()).or_default();
                if number != state.latest() + 1 {
                    return Err(ConfigError::replay_diverged(
                        position,
                        format!("revision {} of target {target}", state.latest() + 1),
                    ));
                }
                state.revisions.push(Revision::new(target, files));
                if activate {
                    state.active = Some(number);
                    state.retired = false;
                }
            }
            ConfigRecord::Activate { target, number } => {
                let state = self
                    .targets
                    .get_mut(target)
                    .filter(|state| state.revision(number).is_some());
                let Some(state) = state else {
                    return Err(ConfigError::replay_diverged(
                        position,
                        format!("the activation of a stored revision of target {target}"),
                    ));
                };
                state.active = Some(number);
                state.retired = false;
            }
            ConfigRecord::Retire { target } => {
                let Some(state) = self.targets.get_mut(target) else {
                    return Err(ConfigError::replay_diverged(
                        position,
                        format!("the retirement of a target that exists, not {target}"),
                    ));
                };
                state.retired = true;
            }
        }
        Ok(())
    }
}
