/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
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

    fn has_operations(&self) -> bool {
        Prepared::has_operations(self)
    }

    fn token(&self) -> Option<&SessionToken> {
        Prepared::token(self)
    }

    fn epoch_floor(&self) -> Option<Epoch> {
        Prepared::epoch_floor(self)
    }

    fn dictionary(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        Prepared::dictionary(self)
    }

    fn starts_build(&self) -> bool {
        Prepared::starts_build(self)
    }

    fn is_drop(&self) -> bool {
        Prepared::is_drop(self)
    }

    fn is_one_frame(&self) -> bool {
        Prepared::is_one_frame(self)
    }

    fn ends_build(&self) -> bool {
        Prepared::build_end(self).is_some()
    }
}
