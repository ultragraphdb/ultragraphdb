/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage simulator: an in-memory [`ultragraphdb_traits_store::Store`] that injects storage faults, a
//! stepped [`ultragraphdb_traits_store::Executor`] and a deterministic [`ultragraphdb_traits_store::ListCache`],
//! all driven by one [`Seed`]; a shadow model that checks the store after every step and every recovery; and the
//! [`Simulation`] that runs a workload over them and stops at the first broken invariant, naming its seed.
//!
//! A crate above the store tests its own operations here, from its own tests: it runs its workload through
//! [`Simulation::run`] and adds its own rules by returning [`SimContext::violation`].
//!
//! ```
//! use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};
//!
//! let first = StoreWorkload::run(Seed::new(7), FaultConfig::mixed(), 200, 2).expect("no invariant broken");
//! let again = StoreWorkload::run(Seed::new(7), FaultConfig::mixed(), 200, 2).expect("no invariant broken");
//! assert_eq!(first, again);
//! ```

mod types;
mod utils;

pub use crate::types::breach::Breach;
pub use crate::types::contents::Contents;
pub use crate::types::corruption::Corruption;
pub use crate::types::drop_stage::DropStage;
pub use crate::types::event::Event;
pub use crate::types::fault::Fault;
pub use crate::types::fault_config::FaultConfig;
pub use crate::types::invariant::Invariant;
pub use crate::types::op_record::OpRecord;
pub use crate::types::run_outcome::RunOutcome;
pub use crate::types::seed::Seed;
pub use crate::types::sim_clock::SimClock;
pub use crate::types::sim_context::SimContext;
pub use crate::types::sim_list_cache::SimListCache;
pub use crate::types::sim_rng::SimRng;
pub use crate::types::sim_scan::SimScan;
pub use crate::types::sim_snapshot::SimSnapshot;
pub use crate::types::sim_store::SimStore;
pub use crate::types::simulation::Simulation;
pub use crate::types::stepped_executor::SteppedExecutor;
pub use crate::types::store_model::StoreModel;
pub use crate::types::store_workload::StoreWorkload;
pub use crate::types::stored_version::StoredVersion;
pub use crate::types::stream::Stream;
pub use crate::types::violation::Violation;
