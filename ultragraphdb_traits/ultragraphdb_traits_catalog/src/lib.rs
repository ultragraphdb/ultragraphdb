/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where a partition's catalog comes from: [`CatalogSource`], which the metadata group implements, answering the
//! catalog version of a namespace in effect at an epoch, or a version by its number, each a [`HeldVersion`] whose
//! contents are opaque bytes the catalog's encoding wrote. A partition holds no catalog key: it loads the versions in
//! effect at its open epochs, and those its untrimmed log names, before it replays. A first write of a predicate no
//! version declares leaves a [`DefaultEntry`], which the partition's host sends to the group as a command no write
//! waits for. Behind the `test-utils` feature, on by default, [`TestCatalogSource`]: an in-memory group a test
//! commits versions to and makes unreachable.
//!
//! ```
//! use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion, TestCatalogSource};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = TestCatalogSource::new();
//! source.commit(HeldVersion::new(0, 2, 40, vec![1, 2, 3]));
//! assert_eq!(source.version_at(0, 39)?, None);
//! assert_eq!(source.version_at(0, 40)?.map(|v| v.version()), Some(2));
//! # Ok(())
//! # }
//! ```

mod traits;
mod types;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::traits::catalog_source::CatalogSource;
pub use crate::types::default_entry::DefaultEntry;
pub use crate::types::held_version::HeldVersion;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::catalog_unreachable::CatalogUnreachable;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_catalog_source::TestCatalogSource;
