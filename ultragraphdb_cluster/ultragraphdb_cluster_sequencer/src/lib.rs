/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer: a state machine over an ordered log that closes epochs and orders multi-partition transactions,
//! with no I/O of its own.
//!
//! The [`Sequencer`] closes epochs in order from 1, each between `epoch_ms_min` and `epoch_ms_max` after the one before
//! ([`SequencerConfig`]) and as soon as `epoch_ms_min` allows when a partition or a waiting read asks; it takes the
//! time from the caller, as a [`ClockReading`]. Each [`Close`] records the epoch, its wall-clock time, the catalog
//! versions it announces ([`CatalogVersion`], taking effect one epoch later) and the session evictions it carries.
//! A multi-partition transaction is placed in the open epoch as a [`SequencedTransaction`]. Every close and every
//! transaction is a [`SequencerRecord`] appended to the log, and acknowledged and delivered only once durable, so the
//! state is a function of the log, rebuilt by [`Sequencer::recover`], or by [`Sequencer::recover_from`] from a
//! [`SequencerSnapshot`] and the rounds after it once [`Sequencer::trim`] has trimmed the log through the snapshot.

mod types;

pub use crate::types::catalog_version::CatalogVersion;
pub use crate::types::clock_reading::ClockReading;
pub use crate::types::close::Close;
pub use crate::types::sequenced_transaction::SequencedTransaction;
pub use crate::types::sequencer::Sequencer;
pub use crate::types::sequencer_config::SequencerConfig;
pub use crate::types::sequencer_record::SequencerRecord;
pub use crate::types::sequencer_snapshot::SequencerSnapshot;
