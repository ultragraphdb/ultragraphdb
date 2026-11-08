/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster whose compute runs on a thread apart from the test's: a frontend and partitions the test plays on its own
//! runtime over real connections, and the crate's compute core and session on a runtime of their own, so a read that never
//! yields to its runtime fails the test's bounded wait rather than holding the test.

pub mod apart_cluster;
pub mod apart_partition;
pub mod apart_reads;
