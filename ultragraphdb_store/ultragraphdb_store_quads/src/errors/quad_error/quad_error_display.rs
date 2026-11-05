/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::quad_error::{QuadError, QuadErrorEnum};

impl fmt::Display for QuadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            QuadErrorEnum::Store(e) => write!(f, "{e}"),
            QuadErrorEnum::Posting(e) => write!(f, "{e}"),
            QuadErrorEnum::Term(e) => write!(f, "{e}"),
            QuadErrorEnum::Key(e) => write!(f, "{e}"),
            QuadErrorEnum::Mapping(e) => write!(f, "{e}"),
            QuadErrorEnum::Catalog(e) => write!(f, "{e}"),
            QuadErrorEnum::Id(e) => write!(f, "{e}"),
            QuadErrorEnum::Corrupted { key, reason } => {
                let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
                write!(f, "Corrupted dictionary at key {hex}: {reason}")
            }
            QuadErrorEnum::NoSequence { prefix } => {
                let name = match prefix {
                    1 => "Resource".to_string(),
                    2 => "Statement".to_string(),
                    p => format!("type {p}"),
                };
                write!(
                    f,
                    "No sequence of {name} is held, and the block source leased no block"
                )
            }
            QuadErrorEnum::PendingFull { capacity } => write!(
                f,
                "{capacity} new IDs await their commit, all the dictionary holds: retry once a commit writes them"
            ),
            QuadErrorEnum::NotOwned { iri, bucket } => write!(
                f,
                "{iri} is of bucket {bucket}, another partition's, and the partition holds no ID for it"
            ),
            QuadErrorEnum::NoOwnedBucket { tenant } => write!(
                f,
                "The partition owns no bucket of tenant {tenant}'s run, so it mints no blank node of it"
            ),
            QuadErrorEnum::SkolemDraws { draws } => {
                write!(
                    f,
                    "{draws} skolem IRIs drawn in turn each named an IRI the dictionary holds"
                )
            }
            QuadErrorEnum::UnknownSkolem { iri } => write!(
                f,
                "{iri} lies under the skolem base, and the dictionary holds no blank node or reifier by it"
            ),
        }
    }
}
