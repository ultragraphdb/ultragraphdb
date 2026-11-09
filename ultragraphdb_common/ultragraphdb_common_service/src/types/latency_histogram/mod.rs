/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The buckets of each power of two: each bucket at or above [`SUB_BUCKETS`] spans a sixteenth of its octave, so a quantile
/// read from it is at most 6.25% above the value it stands for.
const SUB_BUCKETS: u64 = 16;

/// The bits of [`SUB_BUCKETS`].
const SUB_BITS: u32 = 4;

/// The largest value a bucket of its own holds: 2^32 − 1 microseconds, about 71 minutes. A value past it counts in the last
/// bucket, and the largest value seen is kept apart.
const VALUE_MAX: u64 = u32::MAX as u64;

/// Buckets 0 to 15 hold the values 0 to 15 one each; then 16 for each octave from 2^4 to 2^31: 464.
const BUCKETS: usize = 464;

// The last octave, 2^31 to 2^32 − 1, ends at the last bucket.
const _: () = assert!(index_of(VALUE_MAX) == BUCKETS - 1);

/// Microsecond durations counted in buckets fixed at its creation, a sixteenth of an octave wide, for quantiles to within
/// 6.25% without keeping the values: the phases of a role's requests. Data plane: [`LatencyHistogram::record`] allocates nothing;
/// the buckets, 3.7 KiB, are allocated once by [`LatencyHistogram::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatencyHistogram {
    counts: Box<[u64; BUCKETS]>,
    count: u64,
    sum: u64,
    max: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    /// A histogram holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counts: Box::new([0; BUCKETS]),
            count: 0,
            sum: 0,
            max: 0,
        }
    }

    /// Counts `us`.
    pub fn record(&mut self, us: u64) {
        if let Some(bucket) = self.counts.get_mut(index_of(us.min(VALUE_MAX))) {
            *bucket = bucket.saturating_add(1);
        }
        self.count = self.count.saturating_add(1);
        self.sum = self.sum.saturating_add(us);
        self.max = self.max.max(us);
    }

    /// The values counted.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Their mean, 0 when none was counted.
    #[must_use]
    pub fn mean(&self) -> u64 {
        self.sum.checked_div(self.count).unwrap_or(0)
    }

    /// The largest value counted.
    #[must_use]
    pub fn max(&self) -> u64 {
        self.max
    }

    /// The value at or below which `permille` thousandths of the values lie, as the highest value of its bucket and at most the
    /// largest value counted: within 6.25% above the true quantile. 0 when none was counted; `permille` past 1,000 reads as 1,000.
    #[must_use]
    pub fn quantile(&self, permille: u64) -> u64 {
        // The rank, 1-based, of the value asked for: at least the first.
        let rank = (self.count.saturating_mul(permille.min(1_000)))
            .div_ceil(1_000)
            .max(1);
        let mut seen = 0u64;
        for (index, count) in self.counts.iter().enumerate() {
            seen = seen.saturating_add(*count);
            if seen >= rank {
                return highest_of(index).min(self.max);
            }
        }
        0
    }

    /// Counts nothing again, keeping its buckets.
    pub fn clear(&mut self) {
        self.counts.fill(0);
        self.count = 0;
        self.sum = 0;
        self.max = 0;
    }
}

/// The bucket of `value`, at most [`VALUE_MAX`]: the value itself below [`SUB_BUCKETS`], else 16 per octave past the first,
/// placed by the four bits below the highest.
const fn index_of(value: u64) -> usize {
    if value < SUB_BUCKETS {
        // Below 16: lossless.
        return value as usize;
    }
    let octave = u64::BITS - 1 - value.leading_zeros();
    let sub = (value >> (octave - SUB_BITS)) & (SUB_BUCKETS - 1);
    // At most 16 · 28 + 15 = 463 for a value of 32 bits: lossless.
    ((octave - SUB_BITS + 1) as usize) * (SUB_BUCKETS as usize) + sub as usize
}

/// The highest value bucket `index` holds.
fn highest_of(index: usize) -> u64 {
    // At most 463: lossless.
    let index = index as u64;
    if index < SUB_BUCKETS {
        return index;
    }
    let octave = index / SUB_BUCKETS + u64::from(SUB_BITS) - 1;
    let sub = index % SUB_BUCKETS;
    let width = 1u64 << (octave - u64::from(SUB_BITS));
    (SUB_BUCKETS + sub) * width + width - 1
}
