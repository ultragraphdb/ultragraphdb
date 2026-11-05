/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, ScanAt};

/// The one version a Scan reads and the epoch its frames name, chosen once any wait has ended, on a partition that has closed
/// `closed` with the visibility watermark `watermark` (`PROTOCOL.md` §§13.13, 13.14). Nothing reads above the watermark, so a
/// version within a closed epoch reads what was applied at or below it.
///
/// # Errors
///
/// Code 27, `detail` the watermark, for a version above the watermark in an epoch not closed.
pub fn scan_version(
    at: ScanAt,
    read_version: u64,
    closed: Epoch,
    watermark: u64,
) -> Result<(u64, u64), ErrorReply> {
    let bound = Version::bound_of(closed);
    match at {
        ScanAt::ReadVersion | ScanAt::Named => {
            if read_version > bound && read_version > watermark {
                return Err(ErrorReply::new(
                    ErrorCode::AheadOfWatermark,
                    format!(
                        "version {read_version} is above the partition's visibility watermark {watermark}"
                    ),
                )
                .with_detail(watermark));
            }
            let epoch = Version::from_u64(read_version).epoch().get();
            Ok((read_version.min(watermark), epoch))
        }
        ScanAt::Latest => {
            let epoch = Version::from_u64(watermark).epoch().max(closed).get();
            Ok((watermark, epoch))
        }
        ScanAt::LatestClosed => Ok((bound.min(watermark), closed.get())),
    }
}

/// The version the frames of a Scan name, `version` being what it reads (`PROTOCOL.md` §13.14): what it named at `at` 1 and 4, the
/// version bound of `closed` at `at` 3, and the watermark it read at `at` 2. A later Scan naming it reads what this one read.
#[must_use]
pub fn scan_named(at: ScanAt, read_version: u64, closed: Epoch, version: u64) -> u64 {
    match at {
        ScanAt::ReadVersion | ScanAt::Named => read_version,
        ScanAt::LatestClosed => Version::bound_of(closed),
        ScanAt::Latest => version,
    }
}
