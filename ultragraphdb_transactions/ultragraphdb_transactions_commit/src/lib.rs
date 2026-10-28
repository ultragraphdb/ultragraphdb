/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step: rounds, commit versions, exact validation, commit-time operations, the apply, the visibility
//! watermark and sessions.
//!
//! [`CommitStep`] is a step function over its inputs ([`Input`]: commit requests, sync completions, the oldest open
//! read), so the simulator drives it without a runtime. Each step fills a [`StepOutput`]: the round it closed, to
//! append to the log, with its Round body written straight into a reused buffer; the round whose sync completed, to
//! apply, with its admitted proposals at their commit versions; and the replies. Inside, a [`Sequencer`] gives
//! commit versions, `max(previous + 2, 2 × clock µs)`, even, and a [`Resolver`] decides each transaction of a round in
//! round order against the [`ConflictHistory`], exactly.

mod errors;
mod extensions;
mod traits;
mod types;

pub use crate::errors::commit_error::{CommitError, CommitErrorEnum};
pub use crate::traits::proposal::Proposal;
pub use crate::types::commit_request::CommitRequest;
pub use crate::types::commit_step::CommitStep;
pub use crate::types::conflict_history::ConflictHistory;
pub use crate::types::input::Input;
pub use crate::types::reply::Reply;
pub use crate::types::resolver::Resolver;
pub use crate::types::round_bounds::RoundBounds;
pub use crate::types::round_record::RoundRecord;
pub use crate::types::sequencer::Sequencer;
pub use crate::types::step_output::StepOutput;
