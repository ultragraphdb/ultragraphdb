/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go, persistent_hnsw.go and posting/lists.go (the texts of their
 * errors). Changed for ultragraphdb: translated to Rust; texts of its own where Dgraph panics.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::{Display, Formatter, Result};

use ultragraphdb_core_tok::EMPTY_HNSW_TREE_ERROR;

use super::{HnswError, HnswErrorEnum};

impl Display for HnswError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            HnswErrorEnum::Posting(error) => write!(f, "{error}"),
            HnswErrorEnum::Options(error) => write!(f, "{error}"),
            HnswErrorEnum::NoValue => f.write_str("No value found"),
            HnswErrorEnum::KeyNotFound => f.write_str("Key not found"),
            // helper.go: fmt.Errorf("%w: %w; %s", err, errFetchingPostingList, keyString+" with uid "+uid).
            HnswErrorEnum::FetchingPostingList { cause, pred, uid } => {
                write!(
                    f,
                    "{cause}: error fetching posting list; {pred} with uid {uid}"
                )
            }
            // helper.go: fmt.Errorf("%w; %w", errNilVector, err).
            HnswErrorEnum::NilVector(None) => f.write_str("nil vector returned"),
            HnswErrorEnum::NilVector(Some(cause)) => write!(f, "nil vector returned; {cause}"),
            // persistent_hnsw.go: errors.Wrapf(err, EmptyHNSWTreeError).
            HnswErrorEnum::EmptyTree(None) => f.write_str(EMPTY_HNSW_TREE_ERROR),
            HnswErrorEnum::EmptyTree(Some(cause)) => write!(f, "{EMPTY_HNSW_TREE_ERROR}: {cause}"),
            HnswErrorEnum::DifferentLengths(func_name) => {
                write!(
                    f,
                    "can not compute {func_name} on vectors of different lengths"
                )
            }
            HnswErrorEnum::CannotCast(go_type) => {
                write!(f, "cannot cast {go_type} to SimilarityType")
            }
            HnswErrorEnum::UintSlice(s) => write!(f, "Cannot convert {s} to uint slice"),
            HnswErrorEnum::FloatBits { bits, expected } => write!(
                f,
                "a vector index of {expected}-bit floats cannot be created with {bits} float bits"
            ),
            HnswErrorEnum::InvalidParameter { name, value } => {
                write!(
                    f,
                    "the vector index option {name} must be at least 1, got {value}"
                )
            }
            HnswErrorEnum::LevelMissing { uid, level, levels } => write!(
                f,
                "the neighbour lists of uid {uid} hold {levels} levels, level {level} is read"
            ),
            HnswErrorEnum::NoNeighbour => f.write_str(
                "a vector index layer search kept no neighbour: its breadth or result count is 0",
            ),
        }
    }
}
