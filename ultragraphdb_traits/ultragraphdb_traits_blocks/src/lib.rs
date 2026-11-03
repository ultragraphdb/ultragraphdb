/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where a partition's sequences come from: [`BlockSource`], which the metadata group implements, leasing a partition
//! half-open [`SequenceBlock`]s of one type's sequences, per type and partition, never leasing a block twice, so no
//! two partitions allocate one sequence and no sequence is reused. Behind the `test-utils` feature, on by default,
//! [`TestBlockSource`]: a source over an in-memory counter or scripted blocks, which a test can make unreachable.
//!
//! ```
//! use std::num::NonZeroU64;
//!
//! use ultragraphdb_core_ids::TypePrefix;
//! use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock, TestBlockSource};
//!
//! let source = TestBlockSource::new(NonZeroU64::MIN.saturating_add(999));
//! let first = source.lease(TypePrefix::RESOURCE);
//! assert_eq!(first, SequenceBlock::new(1, 1001));
//! assert_eq!(source.lease(TypePrefix::RESOURCE), SequenceBlock::new(1001, 2001));
//! ```

mod traits;
mod types;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::traits::block_source::BlockSource;
pub use crate::types::sequence_block::SequenceBlock;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_block_source::TestBlockSource;
