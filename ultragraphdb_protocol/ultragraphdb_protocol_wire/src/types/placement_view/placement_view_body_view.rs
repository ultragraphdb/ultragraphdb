/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_body::Sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::bucket_range::BucketRange;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::placement::Placement;
use crate::types::placement_view::PlacementView;
use crate::types::preferred_leader::PreferredLeader;
use crate::types::tenant_run::TenantRun;
use crate::utils::body::read_sections;
use crate::utils::command_header::CommandHeader;
use crate::utils::command_header::expect_command;
use crate::utils::fields::epoch;
use crate::utils::whole_records::whole_records;

/// One past the last bucket, 2²⁴: where the last range ends.
const BUCKETS: u32 = 1 << 24;

impl<'a> BodyView<'a> for PlacementView<'a> {
    const COMMAND: Command = Command::Placement;

    type Body = Placement;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        let (position, asked, from, until) = epochs(command, header)?;
        let sections = read_sections::<3>(command, frame.body(), 0)?;
        let (ranges, runs, leaders) = (sections.get(0), sections.get(1), sections.get(2));
        if from == 0 {
            let bytes = ranges.len() + runs.len() + leaders.len();
            if bytes != 0 {
                return Err(ProtocolError::count_mismatch(
                    command,
                    "placement",
                    bytes as u64, // lossless: a `usize` is at most 64 bits
                    0,
                ));
            }
        } else {
            check_map(command, &sections)?;
        }
        Ok(Self {
            position,
            epoch: asked,
            from,
            until,
            ranges,
            runs,
            leaders,
        })
    }

    fn to_body(&self) -> Result<Placement, ProtocolError> {
        let mut map = Placement::reply(self.position, self.epoch, self.from, self.until);
        (0..self.range_count())
            .filter_map(|i| self.range(i))
            .for_each(|r| map.push_range(r));
        (0..self.run_count())
            .filter_map(|i| self.run(i))
            .for_each(|r| map.push_run(r));
        (0..self.leader_count())
            .filter_map(|i| self.leader(i))
            .for_each(|l| map.push_leader(l));
        Ok(map)
    }
}

/// The command header's unused bytes, its epochs, and the request's or the reply's rules on them: `position`, `epoch`,
/// `from` and `until`.
fn epochs(command: u16, header: CommandHeader<'_>) -> Result<(u64, u64, u64, u64), ProtocolError> {
    header.unused_zero(&[(0, 32)])?;
    let position = header.u64_at(0);
    let asked = epoch(command, "epoch", header.u64_at(8), true)?;
    let from = epoch(command, "from", header.u64_at(16), true)?;
    let until = epoch(command, "until", header.u64_at(24), true)?;
    if from == 0 {
        if let Some(value) = [position, until].into_iter().find(|v| *v != 0) {
            return Err(ProtocolError::field_value(command, "placement", value));
        }
    } else if asked != 0 && from > asked {
        return Err(ProtocolError::field_value(command, "placement epoch", from));
    } else if until != 0 && (until <= from || until <= asked) {
        return Err(ProtocolError::field_value(
            command,
            "placement epoch",
            until,
        ));
    }
    Ok((position, asked, from, until))
}

/// A reply's sections: each a whole number of its records, then the ranges, the runs and the leaders in turn.
fn check_map(command: u16, sections: &Sections<'_, 3>) -> Result<(), ProtocolError> {
    let ranges = whole_records::<{ BucketRange::SIZE }>(command, 0, sections.get(0))?;
    let runs = whole_records::<{ TenantRun::SIZE }>(command, 1, sections.get(1))?;
    let leaders = whole_records::<{ PreferredLeader::SIZE }>(command, 2, sections.get(2))?;
    let partitions = check_ranges(command, sections.offset(0), ranges)?;
    check_runs(command, sections.offset(1), runs)?;
    check_leaders(command, leaders, &partitions)
}

/// The ranges cover every bucket once, in order, neighbours naming different partitions; the partitions they name.
fn check_ranges(
    command: u16,
    at: usize,
    ranges: &[[u8; BucketRange::SIZE]],
) -> Result<BTreeSet<u32>, ProtocolError> {
    let mut partitions = BTreeSet::new();
    let (mut end, mut previous) = (0u32, 0u32);
    for (i, bytes) in ranges.iter().enumerate() {
        let range = BucketRange::decode(command, at + BucketRange::SIZE * i, bytes)?;
        if range.start() != end
            || range.end() <= range.start()
            || range.partition() == 0
            || range.partition() == previous
        {
            // Lossless: a `usize` is at most 64 bits.
            return Err(ProtocolError::field_value(command, "range", i as u64));
        }
        (end, previous) = (range.end(), range.partition());
        let _new = partitions.insert(range.partition());
    }
    if end != BUCKETS {
        return Err(ProtocolError::field_value(
            command,
            "range",
            ranges.len() as u64, // lossless: a `usize` is at most 64 bits
        ));
    }
    Ok(partitions)
}

/// The runs come by tenant, each above the one before.
fn check_runs(
    command: u16,
    at: usize,
    runs: &[[u8; TenantRun::SIZE]],
) -> Result<(), ProtocolError> {
    let mut previous: Option<u64> = None;
    for (i, bytes) in runs.iter().enumerate() {
        let run = TenantRun::decode(command, at + TenantRun::SIZE * i, bytes)?;
        if previous.is_some_and(|before| run.tenant() <= before) {
            return Err(ProtocolError::field_value(command, "run", i as u64)); // lossless: a `usize` is at most 64 bits
        }
        previous = Some(run.tenant());
    }
    Ok(())
}

/// The leaders come by partition, each of a replica, one per partition the ranges name.
fn check_leaders(
    command: u16,
    leaders: &[[u8; PreferredLeader::SIZE]],
    partitions: &BTreeSet<u32>,
) -> Result<(), ProtocolError> {
    let mut previous = 0u32;
    for (i, bytes) in leaders.iter().enumerate() {
        let leader = PreferredLeader::decode(*bytes);
        if leader.partition() <= previous || leader.replica() == 0 {
            // Lossless: a `usize` is at most 64 bits.
            return Err(ProtocolError::field_value(command, "leader", i as u64));
        }
        previous = leader.partition();
    }
    let named = leaders
        .iter()
        .map(|bytes| PreferredLeader::decode(*bytes).partition());
    if !named.eq(partitions.iter().copied()) {
        return Err(ProtocolError::count_mismatch(
            command,
            "leaders",
            // The casts are lossless: a `usize` is at most 64 bits.
            leaders.len() as u64,
            partitions.len() as u64,
        ));
    }
    Ok(())
}
