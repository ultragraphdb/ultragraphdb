/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The client endpoint's scenarios, bound to `features/`: a frontend's listeners on the test's thread, the system group on
//! its own and the other roles played by doubles.

mod declaring_steps;
mod endpoint_stage;
mod endpoint_steps;
mod endpoint_steps_requests;
mod endpoint_steps_sessions;
mod on_way_steps;
mod operation_steps;
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
