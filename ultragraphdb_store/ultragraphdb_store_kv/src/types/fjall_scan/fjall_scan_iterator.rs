/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Direction, Item};

use crate::types::fjall_scan::FjallScan;

impl Iterator for FjallScan {
    type Item = Result<Item, StoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.pending.pop_front() {
                if item.is_err() {
                    self.pending.clear();
                    self.iter = None;
                    self.carry = None;
                }
                return Some(item);
            }
            if self.iter.is_none() && self.carry.is_none() {
                return None;
            }
            match self.scan.direction() {
                Direction::Forward => match self.read(false)? {
                    Ok((key, version, bytes)) => self.consume(key, version, &bytes),
                    Err(error) => self.pending.push_back(Err(error)),
                },
                Direction::Reverse => match self.read_back_key() {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(error) => self.pending.push_back(Err(error)),
                },
            }
        }
    }
}
