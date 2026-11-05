/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{List, PostingError};

use crate::errors::quad_error::QuadError;
use crate::utils::postings::posting_object;

/// The objects `list`, the list of `key`, holds at `read_ts`, each with its statement IDs.
///
/// # Errors
///
/// Corruption of the list.
pub fn list_objects(
    key: &[u8],
    list: &List,
    read_ts: u64,
) -> Result<Vec<(EncodedTerm, Vec<SystemId>)>, QuadError> {
    let mut objects = Vec::new();
    list.iterate(read_ts, 0, |p| {
        match posting_object(key, p) {
            Ok(object) => objects.push((object, p.statement_ids().to_vec())),
            Err(e) => return Err(PostingErrorOr::Quad(e)),
        }
        Ok(ControlFlow::Continue(()))
    })
    .map_err(PostingErrorOr::into_quad)?;
    Ok(objects)
}

/// A list iteration's error: the posting crate's, or the quad store's own reading a posting.
enum PostingErrorOr {
    Posting(PostingError),
    Quad(QuadError),
}

impl From<PostingError> for PostingErrorOr {
    fn from(e: PostingError) -> Self {
        Self::Posting(e)
    }
}

impl PostingErrorOr {
    fn into_quad(self) -> QuadError {
        match self {
            Self::Posting(e) => e.into(),
            Self::Quad(e) => e,
        }
    }
}
