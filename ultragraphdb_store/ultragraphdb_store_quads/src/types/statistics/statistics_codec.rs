/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::errors::quad_error::QuadError;
use crate::types::statistics::{SKETCH_REGISTERS, Statistics};

/// The format of a statistics value: a version byte, the statement count as an i64 big-endian, the kinds as a u16
/// count of a u16-length name and an i64 count each, then each sketch's registers.
const FORMAT: u8 = 1;

impl Statistics {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = vec![FORMAT];
        w.extend_from_slice(&self.statements.to_be_bytes());
        w.extend_from_slice(&(self.kinds.len() as u16).to_be_bytes());
        for (kind, n) in &self.kinds {
            w.extend_from_slice(&(kind.len() as u16).to_be_bytes());
            w.extend_from_slice(kind.as_bytes());
            w.extend_from_slice(&n.to_be_bytes());
        }
        w.extend_from_slice(&self.subjects);
        w.extend_from_slice(&self.objects);
        w
    }

    /// The statistics stored as `bytes` under `key`.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, QuadError> {
        let mut r = Reader { key, bytes, at: 0 };
        if r.take(1)?[0] != FORMAT {
            return Err(r.bad());
        }
        let statements = r.i64()?;
        let mut kinds = BTreeMap::new();
        for _ in 0..r.u16()? {
            let len = usize::from(r.u16()?);
            let name = String::from_utf8(r.take(len)?.to_vec()).map_err(|_| r.bad())?;
            kinds.insert(name, r.i64()?);
        }
        let mut subjects = [0u8; SKETCH_REGISTERS];
        subjects.copy_from_slice(r.take(SKETCH_REGISTERS)?);
        let mut objects = [0u8; SKETCH_REGISTERS];
        objects.copy_from_slice(r.take(SKETCH_REGISTERS)?);
        if r.at != bytes.len() {
            return Err(r.bad());
        }
        Ok(Self {
            statements,
            kinds,
            subjects,
            objects,
        })
    }
}

struct Reader<'a> {
    key: &'a [u8],
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bad(&self) -> QuadError {
        QuadError::Corrupted(self.key, "unreadable statistics")
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], QuadError> {
        let part = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| self.bad())?;
        self.at += n;
        Ok(part)
    }

    fn u16(&mut self) -> Result<u16, QuadError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i64(&mut self) -> Result<i64, QuadError> {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(self.take(8)?);
        Ok(i64::from_be_bytes(raw))
    }
}
