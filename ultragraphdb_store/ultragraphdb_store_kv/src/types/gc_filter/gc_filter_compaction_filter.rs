/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use fjall::compaction::filter::{
    CompactionFilter, CompactionFilterResult, Context, ItemAccessor, Verdict,
};

use crate::types::gc_filter::GcFilter;

impl CompactionFilter for GcFilter {
    /// The verdict of [`GcFilter::verdict`]. A value fjall cannot read is kept rather than reported: an error would
    /// abort the compaction and poison the database.
    fn filter_item(&mut self, item: ItemAccessor<'_>, _ctx: &Context) -> CompactionFilterResult {
        let key = item.key().clone();
        Ok(match item.value() {
            Ok(value) => self.verdict(&key, &value),
            Err(error) => {
                tracing::warn!(error = %error, "compaction filter could not read a value and kept it");
                self.undecodable += 1;
                Verdict::Keep
            }
        })
    }

    /// Hands the counts and the collected keys to the store, which journals a removal of each.
    fn finish(self: Box<Self>) {
        let filter = *self;
        if filter.undecodable > 0 {
            tracing::warn!(
                undecodable = filter.undecodable,
                last_level = filter.last_level,
                "compaction kept items it could not decode"
            );
        }
        if let Some(settings) = &filter.settings {
            settings.add_undecodable(filter.undecodable);
            settings.add_collected(filter.collected);
        }
    }
}
