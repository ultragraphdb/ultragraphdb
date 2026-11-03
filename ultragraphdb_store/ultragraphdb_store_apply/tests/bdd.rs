/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios run over a changeset applier on the simulated store: in `changeset_steps`, those of changesets,
//! index maintenance, posting lists, the catalog's write checks, schema storage's writes and statement IDs; in
//! `catalog_change_steps`, those of index builds, the declarations data refuses and drops; in `transaction_steps`, the
//! recording side of what a transaction's reads record; in `vector_steps`, those of the vector index's lists and
//! maintenance; in `schema_storage_steps`, the catalog versions a partition loads and the default entries it asks for.

use std::sync::Arc;

use cucumber::World;

/// The unit tests' helpers, shared by path.
pub mod utils {
    #[path = "changesets.rs"]
    pub mod changesets;
    #[path = "probe.rs"]
    pub mod probe;
}

mod catalog_change_steps;
mod changeset_steps;
mod schema_storage_steps;
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
    statement: Option<ultragraphdb_core_ids::SystemId>,
    events: Vec<ultragraphdb_store_sim::CapturedEvent>,
    /// The metadata group the partition loads its catalog versions from and sends its default entries to.
    source: ultragraphdb_traits_catalog::TestCatalogSource,
    /// The catalog held before a restart, and the refusals a catalog version met.
    catalogs: Vec<ultragraphdb_core_catalog::Catalog>,
    refusals: Vec<String>,
}

fn main() -> ultragraphdb_tool_bdd::TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
