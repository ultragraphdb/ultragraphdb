/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use crate::errors::partition_error::{PartitionError, PartitionErrorEnum};

impl Error for PartitionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.0 {
            PartitionErrorEnum::Txn(e) => Some(e),
            PartitionErrorEnum::Changeset(e) => Some(e),
            PartitionErrorEnum::Quad(e) => Some(e),
            PartitionErrorEnum::Pipeline(e) => Some(e),
            PartitionErrorEnum::Commit(e) => Some(e),
            PartitionErrorEnum::WrongPartition { .. }
            | PartitionErrorEnum::CrossPartition { .. }
            | PartitionErrorEnum::TimedOut { .. }
            | PartitionErrorEnum::Refused { .. } => None,
        }
    }
}
