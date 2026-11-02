/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! [`ClusterError`], why an operation of the cluster's state machines was refused: the placement map, the metadata
//! group, the sequencer and the partitions raise it, so each depends on this crate alone for it. It carries the
//! ordered log's [`LogError`](ultragraphdb_error_log::LogError) and an
//! [`InvariantError`](ultragraphdb_error_invariant::InvariantError), into which `?` converts both.

mod errors;

pub use crate::errors::cluster_error::{ClusterError, ClusterErrorEnum};
