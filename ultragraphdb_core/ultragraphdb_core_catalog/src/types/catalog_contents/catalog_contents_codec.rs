/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The encoding of a catalog version's contents, the bytes the metadata group stores (all integers big-endian):
//!
//! | Offset | Field |
//! |---|---|
//! | 0 | format, u8, 1 |
//! | 1 | tenant, u64 |
//! | 9 | the version's number, u64 |
//! | 17 | the number of declarations, u32 |
//! | 21 | each declaration: its length, u32, then its catalog entry's value (`crate::encode_declaration`) |
//!
//! Nothing follows the last declaration.

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};

use crate::errors::catalog_error::{CatalogError, CatalogErrorEnum};
use crate::types::catalog_contents::CatalogContents;
use crate::utils::catalog_codec::{check_lengths, decode, encode};

/// The contents' format byte.
pub(crate) const CONTENTS_FORMAT: u8 = 1;

/// The length of the fixed part before the first declaration: format, tenant, version and count.
const HEADER_LEN: usize = 21;

const _: () = assert!(HEADER_LEN == 1 + 8 + 8 + 4);

/// Why a declaration of the contents does not decode: the reason of the entry decoder's corruption, without the empty
/// key it names.
fn reason_of(error: &CatalogError) -> String {
    if let CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { reason, .. })) = &error.0
    {
        reason.clone()
    } else {
        error.to_string()
    }
}

impl CatalogContents {
    /// The bytes the metadata group stores for the version.
    ///
    /// # Errors
    ///
    /// [`CatalogError::too_large`] when a declaration's string or list is longer than the encoding holds, or the
    /// version holds more than `u32::MAX` declarations.
    pub fn encode(&self) -> Result<Vec<u8>, CatalogError> {
        let count = u32::try_from(self.declarations.len()).map_err(|_| {
            CatalogError::too_large(
                "",
                "declarations",
                self.declarations.len(),
                u32::MAX as usize,
            )
        })?;
        let mut w = Vec::with_capacity(HEADER_LEN);
        w.push(CONTENTS_FORMAT);
        w.extend_from_slice(&self.tenant.to_be_bytes());
        w.extend_from_slice(&self.version.to_be_bytes());
        w.extend_from_slice(&count.to_be_bytes());
        debug_assert_eq!(w.len(), HEADER_LEN);
        for declaration in &self.declarations {
            check_lengths(declaration)?;
            let entry = encode(declaration);
            let len = u32::try_from(entry.len()).map_err(|_| {
                CatalogError::too_large("", "declaration bytes", entry.len(), u32::MAX as usize)
            })?;
            w.extend_from_slice(&len.to_be_bytes());
            w.extend_from_slice(&entry);
        }
        // Paired with the load: what the group is given reads back as these contents.
        debug_assert!(Self::decode(self.tenant, self.version, &w).is_ok_and(|read| read == *self));
        Ok(w)
    }

    /// The contents `bytes` hold, read as version `version` of `tenant`'s catalog.
    ///
    /// # Errors
    ///
    /// [`CatalogError::corrupted_version`], naming the tenant and version, when the bytes are not of the format,
    /// end early or run on, name another tenant or version, or hold a declaration that does not decode.
    pub fn decode(tenant: u64, version: u64, bytes: &[u8]) -> Result<Self, CatalogError> {
        let corrupted = |reason: String| CatalogError::corrupted_version(tenant, version, reason);
        let ends_early = || corrupted("the contents end early".to_string());
        let (header, mut rest) = bytes.split_at_checked(HEADER_LEN).ok_or_else(ends_early)?;
        let (format, header) = header.split_first().ok_or_else(ends_early)?;
        if *format != CONTENTS_FORMAT {
            return Err(corrupted(format!("unknown format {format}")));
        }
        let (named_tenant, header) = header.split_first_chunk::<8>().ok_or_else(ends_early)?;
        let (named_version, header) = header.split_first_chunk::<8>().ok_or_else(ends_early)?;
        let (count, _) = header.split_first_chunk::<4>().ok_or_else(ends_early)?;
        let (named_tenant, named_version) = (
            u64::from_be_bytes(*named_tenant),
            u64::from_be_bytes(*named_version),
        );
        if named_tenant != tenant {
            return Err(corrupted(format!(
                "the contents name tenant {named_tenant}"
            )));
        }
        if named_version != version {
            return Err(corrupted(format!(
                "the contents name version {named_version}"
            )));
        }
        // Each declaration takes at least its 4-byte length, so the loop ends within the bytes whatever the count
        // says, and nothing is allocated for a count the bytes do not hold.
        let mut declarations = Vec::new();
        for _ in 0..u32::from_be_bytes(*count) {
            let (len, after) = rest.split_first_chunk::<4>().ok_or_else(ends_early)?;
            let len = usize::try_from(u32::from_be_bytes(*len)).map_err(|_| ends_early())?;
            let (entry, after) = after.split_at_checked(len).ok_or_else(ends_early)?;
            declarations.push(decode(&[], entry).map_err(|e| corrupted(reason_of(&e)))?);
            rest = after;
        }
        if !rest.is_empty() {
            return Err(corrupted("bytes follow the last declaration".to_string()));
        }
        Ok(Self::new(tenant, version, declarations))
    }
}
