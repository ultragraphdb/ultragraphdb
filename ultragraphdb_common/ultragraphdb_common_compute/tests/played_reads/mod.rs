/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Reads run in this process against played roles: compute's core over real connections to a played frontend and played
//! partitions, each answering from a table of keys, so a test states what the partitions hold and sees every Scan compute sends.

pub mod played_cluster;
pub mod played_data;
pub mod played_frontend;
pub mod played_partition;
pub mod read_answer;
