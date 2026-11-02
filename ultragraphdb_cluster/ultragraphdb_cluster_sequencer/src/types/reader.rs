/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;

/// A reader of big-endian values from encoded bytes, each refused as [`ClusterError::malformed`] naming what it should
/// have been and its offset when the bytes end first.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the first of `bytes`.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// The offset of the next value.
    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    /// The next `N` bytes, naming `what` when fewer are left.
    fn take<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N], ClusterError> {
        let rest = self.bytes.get(self.offset..).unwrap_or_default();
        let (taken, _) = rest
            .split_first_chunk::<N>()
            .ok_or_else(|| ClusterError::malformed(what, self.offset))?;
        self.offset += N;
        Ok(*taken)
    }

    /// The next byte.
    pub(crate) fn u8(&mut self, what: &'static str) -> Result<u8, ClusterError> {
        self.take::<1>(what).map(|[byte]| byte)
    }

    /// The next 16 bytes as a number.
    pub(crate) fn u128(&mut self, what: &'static str) -> Result<u128, ClusterError> {
        self.take(what).map(u128::from_be_bytes)
    }

    /// The next `len` bytes.
    pub(crate) fn bytes(
        &mut self,
        len: usize,
        what: &'static str,
    ) -> Result<&'a [u8], ClusterError> {
        let taken = self
            .bytes
            .get(self.offset..)
            .and_then(|rest| rest.get(..len))
            .ok_or_else(|| ClusterError::malformed(what, self.offset))?;
        self.offset += len;
        Ok(taken)
    }

    /// The next 2 bytes as a number.
    pub(crate) fn u16(&mut self, what: &'static str) -> Result<u16, ClusterError> {
        self.take(what).map(u16::from_be_bytes)
    }

    /// The next 4 bytes as a number.
    pub(crate) fn u32(&mut self, what: &'static str) -> Result<u32, ClusterError> {
        self.take(what).map(u32::from_be_bytes)
    }

    /// The next 8 bytes as a number.
    pub(crate) fn u64(&mut self, what: &'static str) -> Result<u64, ClusterError> {
        self.take(what).map(u64::from_be_bytes)
    }

    /// Refuses bytes after the last value read.
    pub(crate) fn finish(&self) -> Result<(), ClusterError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ClusterError::malformed(
                "the end of the record",
                self.offset,
            ))
        }
    }
}
