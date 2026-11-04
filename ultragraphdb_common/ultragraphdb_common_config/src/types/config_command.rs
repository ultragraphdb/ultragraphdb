/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;

use crate::errors::config_error::ConfigError;
use crate::types::target_summary::TargetSummary;

/// What the API asks of the task owning the store.
#[derive(Debug)]
pub(crate) enum ConfigCommand {
    Targets(Sender<Vec<TargetSummary>>),
    Instance {
        target: String,
        revision: Option<u64>,
        instance: String,
        reply: Sender<Result<(u64, String), ConfigError>>,
    },
    Files {
        target: String,
        revision: u64,
        reply: Sender<Result<Vec<u8>, ConfigError>>,
    },
    Add {
        target: String,
        base: u64,
        body: Vec<u8>,
        reply: Sender<Result<u64, ConfigError>>,
    },
    Activate {
        target: String,
        revision: u64,
        reply: Sender<Result<(), ConfigError>>,
    },
    Retire {
        target: String,
        reply: Sender<Result<(), ConfigError>>,
    },
}
