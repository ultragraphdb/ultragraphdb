/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use fjall::compaction::filter::{CompactionFilter, Context, Factory};

use crate::types::gc_filter::GcFilter;
use crate::types::gc_settings::GcSettings;

/// Makes the [`GcFilter`] of each compaction of the data keyspace, under the discard timestamp in effect when the
/// compaction starts. fjall's API takes it as an `Arc<dyn Factory>` returning a `Box<dyn CompactionFilter>`, the one
/// place this crate dispatches dynamically.
#[derive(Debug)]
pub struct GcFilterFactory {
    settings: Arc<GcSettings>,
}

impl GcFilterFactory {
    pub fn new(settings: Arc<GcSettings>) -> Self {
        Self { settings }
    }
}

impl Factory for GcFilterFactory {
    fn name(&self) -> &str {
        "ultragraphdb_discard"
    }

    fn make_filter(&self, ctx: &Context) -> Box<dyn CompactionFilter> {
        Box::new(GcFilter::for_store(
            self.settings.clone(),
            ctx.is_last_level,
        ))
    }
}
