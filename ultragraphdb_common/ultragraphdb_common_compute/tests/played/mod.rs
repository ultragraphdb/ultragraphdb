/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster played on the test's one runtime, which a read runs through as it does between processes: a frontend and
//! partitions the test plays over real connections, and the crate's own compute core and sessions between them.

pub mod played_cluster;
pub mod played_data;
pub mod played_frontend;
pub mod played_partition;
pub mod played_read;
