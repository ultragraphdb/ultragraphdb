/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The client endpoint's scenarios, bound to `features/`: compute's listeners on the test's thread, the system group on
//! its own and the other roles played by doubles.

// The played cluster of the unit tests, which the executor's scenarios read through, included by path.
#[path = "apart/mod.rs"]
pub mod apart;
mod declaring_steps;
mod endpoint_stage;
mod endpoint_steps;
mod endpoint_steps_requests;
mod endpoint_steps_sessions;
mod executor_steps;
mod link_steps;
mod on_way_steps;
mod operation_steps;
mod played_stage;
mod relay_steps;
mod tightening_steps;

use cucumber::World;
use ultragraphdb_tool_bdd::TestResult;

#[derive(Debug, Default, World)]
struct Steps {
    ran: bool,
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
