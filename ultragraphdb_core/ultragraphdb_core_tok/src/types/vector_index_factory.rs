/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index_factory.go (indexFactory) and tok/hnsw/persistent_factory.go and
 * persistent_hnsw.go (the factory's name, AllowedOptions and GetOptions). Changed for ultragraphdb: translated to
 * Rust; the registry entry without the index, which stage 2 ports.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_error_options::OptionsError;
use ultragraphdb_traits_index::{AllowedOptions, Metric, OptionValue, Options};

use crate::utils::hnsw::{
    EF_CONSTRUCTION_OPT, EF_SEARCH_OPT, EXPONENT_OPT, HNSW, MAX_LEVELS_OPT, METRIC_OPT,
};

/// The vector index factory Dgraph registers as a tokenizer: `hnsw`, over float32 vectors. It names the factory,
/// validates its options and prints them; the index it creates is stage 2's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexFactory {
    float_bits: usize,
}

impl VectorIndexFactory {
    /// `hnsw.CreateFactory[float32](32)`.
    pub(crate) fn hnsw() -> Self {
        Self { float_bits: 32 }
    }

    /// The factory's name, `hnsw`.
    pub fn name(&self) -> &'static str {
        HNSW
    }

    /// The width in bits of the vectors' floats.
    pub fn float_bits(&self) -> usize {
        self.float_bits
    }

    /// The factory's `AllowedOptions`: `exponent`, `maxLevels`, `efConstruction` and `efSearch` as ints, and
    /// `metric` as one of `euclidean`, `cosine` and `dotproduct`.
    pub fn allowed_options(&self) -> AllowedOptions {
        let mut ao = AllowedOptions::new();
        ao.add_int_option(EXPONENT_OPT)
            .add_int_option(MAX_LEVELS_OPT)
            .add_int_option(EF_CONSTRUCTION_OPT)
            .add_int_option(EF_SEARCH_OPT)
            .add_custom_option(METRIC_OPT, parse_metric);
        ao
    }

    /// The factory's `GetOptions` (`GetPersistantOptions`): the set options as `("name":"value",...)`, in the
    /// order exponent, maxLevels, efConstruction, efSearch, metric; empty when none is set. An int option holding
    /// another type prints Go's default 3, and a metric option holding another type prints an empty name, as Go's
    /// do.
    pub fn get_options(&self, o: &Options) -> String {
        let mut sb = String::new();
        for name in [
            EXPONENT_OPT,
            MAX_LEVELS_OPT,
            EF_CONSTRUCTION_OPT,
            EF_SEARCH_OPT,
        ] {
            if o.specifies(name) {
                let v = o.get_opt::<i64>(name).ok().flatten().unwrap_or(3);
                sb.push_str(&format!("\"{name}\":\"{v}\","));
            }
        }
        if let Some(v) = o.get_interface_opt(METRIC_OPT) {
            let name = match v {
                OptionValue::Metric(m) => m.name(),
                _ => "",
            };
            sb.push_str(&format!("\"{METRIC_OPT}\":\"{name}\","));
        }
        if sb.is_empty() {
            return sb;
        }
        sb.pop();
        format!("({sb})")
    }
}

/// The hnsw factory's metric parser.
fn parse_metric(v: &str) -> Result<OptionValue, OptionsError> {
    Metric::from_name(v)
        .map(OptionValue::Metric)
        .ok_or_else(|| OptionsError::invalid(format!("Can't create a vector index for {v}")))
}
