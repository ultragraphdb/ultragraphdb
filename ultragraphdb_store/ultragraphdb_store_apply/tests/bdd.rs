/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios run over a changeset applier on the simulated store: in `changeset_steps`, those of changesets,
//! index maintenance, posting lists, the catalog's write checks, schema storage's writes and statement IDs; in
//! `catalog_change_steps`, those of index builds, the declarations data refuses and drops; in `transaction_steps`, the
//! recording side of what a transaction's reads record; in `vector_steps`, those of the vector index's lists and
//! maintenance.

use std::sync::Arc;

use cucumber::World;

// The unit tests' helpers, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "changesets.rs"]
    pub mod changesets;
    #[allow(dead_code)]
    #[path = "probe.rs"]
    pub mod probe;
}

mod catalog_change_steps;
mod changeset_steps;
mod transaction_steps;
mod vector_steps;

#[derive(Debug, Default, World)]
struct Steps {
    keys: Vec<Vec<u8>>,
    flags: Vec<bool>,
    schemas: Vec<String>,
    changesets: Option<Arc<crate::utils::changesets::TestChangesets>>,
    copies: Vec<Arc<crate::utils::changesets::TestChangesets>>,
    results:
        Vec<Result<ultragraphdb_store_apply::Applied, ultragraphdb_store_apply::ChangesetError>>,
    names: Vec<String>,
    counts: Vec<Vec<u32>>,
    statement: Option<u64>,
    events: Vec<ultragraphdb_store_sim::CapturedEvent>,
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
