/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step: rounds, commit versions, exact validation, commit-time operations, the apply, the visibility
//! watermark and sessions.
//!
//! [`CommitStep`] is a step function over its inputs ([`Input`]: commit requests, registrations a home accepted, the
//! sequencer's closes, sync completions, the clock, the oldest open read), so the simulator drives it without a runtime.
//! Each step fills a [`StepOutput`]: the round it closed, to append to the log, with its Round body written straight
//! into a reused buffer; the round whose sync completed, to apply, with its admitted proposals at their commit versions;
//! a close to ask of the sequencer when the open epoch is full; and the replies, each commit's with its session token.
//! Inside, an [`EpochStamp`] gives commit versions, the open epoch one above the latest delivered close and an even
//! sequence within it, and a [`Resolver`] decides each transaction of a round in round order against the
//! [`ConflictHistory`], exactly. A [`Pipeline`] drives the step on one node: each closed round
//! appended to the ordered log and synced, each synced round applied to the store, the watermark moved past it, and
//! the replies handed out.

mod errors;
mod extensions;
mod traits;
mod types;

pub use crate::errors::commit_error::{CommitError, CommitErrorEnum};
pub use crate::errors::pipeline_error::{PipelineError, PipelineErrorEnum};
pub use crate::errors::recovery_error::{RecoveryError, RecoveryErrorEnum};
pub use crate::errors::setting_error::{SettingError, SettingErrorEnum};
pub use crate::traits::operator::Operator;
pub use crate::traits::proposal::Proposal;
pub use crate::types::applier_operator::ApplierOperator;
pub use crate::types::close_request::CloseRequest;
pub use crate::types::commit_request::CommitRequest;
pub use crate::types::commit_step::CommitStep;
pub use crate::types::conflict_history::ConflictHistory;
pub use crate::types::epoch_stamp::EpochStamp;
pub use crate::types::input::Input;
pub use crate::types::no_operations::NoOperations;
pub use crate::types::pipeline::Pipeline;
pub use crate::types::replay::Replay;
pub use crate::types::reply::Reply;
pub use crate::types::resolver::Resolver;
pub use crate::types::round_bounds::RoundBounds;
pub use crate::types::round_record::RoundRecord;
pub use crate::types::session_entry::SessionEntry;
pub use crate::types::sessions::Sessions;
pub use crate::types::step_output::StepOutput;
pub use crate::types::stored_reply::StoredReply;
