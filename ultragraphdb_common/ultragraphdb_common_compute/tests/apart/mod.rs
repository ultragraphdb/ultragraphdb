/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A played cluster: a system group and partitions the test plays on its own runtime over real connections, and beside them the
//! crate's compute core, whose reads run by the executor a client's session hands them to.

pub mod apart_cluster;
pub mod apart_executor;
pub mod apart_link;
pub mod apart_partition;
pub mod apart_reads;
pub mod apart_session;
pub mod apart_tap;
