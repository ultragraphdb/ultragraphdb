/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_txn::{Prepared, ReadSet};

use crate::traits::proposal::Proposal;

/// A worker's prepared transaction, as the commit step takes it.
impl Proposal for Prepared {
    fn read_set(&self) -> &ReadSet {
        Prepared::read_set(self)
    }

    fn record(&self) -> &[u8] {
        Prepared::record(self)
    }
}
