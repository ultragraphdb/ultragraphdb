/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use crate::errors::cluster_error::{ClusterError, ClusterErrorEnum};

impl Error for ClusterError {
    /// The carried log or invariant error; `None` for a refusal of the cluster's own.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.0 {
            ClusterErrorEnum::Log(error) => Some(error),
            ClusterErrorEnum::Invariant(error) => Some(error),
            ClusterErrorEnum::ZeroPartition
            | ClusterErrorEnum::PartitionCount { .. }
            | ClusterErrorEnum::BucketRange { .. }
            | ClusterErrorEnum::Unassigned { .. }
            | ClusterErrorEnum::AssignedTwice { .. }
            | ClusterErrorEnum::ChangeEpoch { .. }
            | ClusterErrorEnum::UnknownPartition { .. }
            | ClusterErrorEnum::SettingBelow { .. }
            | ClusterErrorEnum::SettingAbove { .. }
            | ClusterErrorEnum::EpochLimit { .. }
            | ClusterErrorEnum::EpochAhead { .. }
            | ClusterErrorEnum::BeforeFirstClose { .. }
            | ClusterErrorEnum::NoPartitions
            | ClusterErrorEnum::DuplicatePartition { .. }
            | ClusterErrorEnum::NotSubscribed { .. }
            | ClusterErrorEnum::CatchUpTrimmed { .. }
            | ClusterErrorEnum::TooMany { .. }
            | ClusterErrorEnum::Full { .. }
            | ClusterErrorEnum::Malformed { .. }
            | ClusterErrorEnum::ReplayDiverged { .. } => None,
        }
    }
}
