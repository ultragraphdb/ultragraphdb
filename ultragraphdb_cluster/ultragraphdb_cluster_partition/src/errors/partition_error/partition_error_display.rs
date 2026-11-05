/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::partition_error::{PartitionError, PartitionErrorEnum};

impl fmt::Display for PartitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            PartitionErrorEnum::WrongPartition { bucket, owner } => {
                write!(f, "Bucket {bucket} is partition {owner}'s")
            }
            PartitionErrorEnum::CrossPartition { partition, other } => write!(
                f,
                "A request of partition {partition} reaches partition {other}: cross-partition work is refused in this stage"
            ),
            PartitionErrorEnum::TimedOut { partition } => write!(
                f,
                "The read timed out waiting for partition {partition} to close its epoch"
            ),
            PartitionErrorEnum::Refused { outcome } => {
                write!(
                    f,
                    "The home refused the registration with error code {outcome}"
                )
            }
            PartitionErrorEnum::TransactionHeld { identity } => write!(
                f,
                "The partition holds transaction {:032x}",
                u128::from_be_bytes(*identity)
            ),
            PartitionErrorEnum::Txn(e) => write!(f, "{e}"),
            PartitionErrorEnum::Changeset(e) => write!(f, "{e}"),
            PartitionErrorEnum::Quad(e) => write!(f, "{e}"),
            PartitionErrorEnum::Pipeline(e) => write!(f, "{e}"),
            PartitionErrorEnum::Commit(e) => write!(f, "{e}"),
        }
    }
}
