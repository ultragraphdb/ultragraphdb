/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A transaction on a worker: its snapshot at the visibility watermark, its read set, its write-set record and
//! its limits.
//!
//! A [`Transaction`] begins at the watermark W, or at an earlier version it names; a version above W is refused at
//! once. Its writes are changesets executed against its snapshot as they arrive, through one execution of the
//! changeset applier held open, so each later request reads them; its reads are recorded. Its commit hands the commit
//! step a [`Prepared`]: the execution, the sealed [`ReadSet`] and the write-set record [`WriteSetBuilder`] writes in
//! `ultragraphdb_protocol_wire`'s layout. A worker lends every transaction the same [`TxnBuffers`], so its steady
//! state allocates no read set and no record buffer.

mod errors;
mod types;

pub use crate::errors::txn_error::{TxnError, TxnErrorEnum};
pub use crate::types::mode::Mode;
pub use crate::types::operated::Operated;
pub use crate::types::prepared::Prepared;
pub use crate::types::read_set::ReadSet;
pub use crate::types::transaction::{Commit, Transaction};
pub use crate::types::txn_buffers::TxnBuffers;
pub use crate::types::txn_limits::{MAX_FRAME, MAX_WRITE_SET, TxnLimits};
pub use crate::types::write_set_builder::WriteSetBuilder;
