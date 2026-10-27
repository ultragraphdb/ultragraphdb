/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The binary protocol's bytes: the 256-byte header, XXH3-64 checksums and chunk tables, the commands, and every
//! body's layout with its view and builder. No I/O.
//!
//! A frame is read in the order the protocol checks it: [`HeaderView::new`] takes the header's 256 bytes (the
//! length), [`HeaderView::verify`] checks its checksum, command, reserved bytes, cluster and size, and
//! [`FrameView::validate`] does both and then checks the body's chunk table and chunks. A frame is built with
//! [`Header::encode_frame`], which appends the chunk table and fills in `size` and the checksums.

mod types;
mod utils;

pub use crate::types::chunk_layout::ChunkLayout;
pub use crate::types::chunked_body::ChunkedBody;
pub use crate::types::command::Command;
pub use crate::types::frame_view::FrameView;
pub use crate::types::header::Header;
pub use crate::types::header_view::HeaderView;
pub use crate::types::round_header::RoundHeader;
pub use crate::utils::checksum::{xxh3_64, xxh3_64_with_seed};
pub use crate::utils::constants::{
    CHUNK_SIZE, COMMAND_HEADER_SIZE, FIRST_CHUNK_SIZE, HEADER_SIZE, LEAF_SIZE, MAX_FRAME_SIZE,
    PROTOCOL_VERSION, REPLICA_COMMANDS, RETIRED_COMMANDS,
};
