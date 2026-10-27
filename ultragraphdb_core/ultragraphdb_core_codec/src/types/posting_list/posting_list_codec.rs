/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting format: how a posting list is stored. Little-endian throughout:
//!
//! - a version byte, 1;
//! - a flags byte: bit 0 set when the list has a pack, every other bit clear;
//! - the commit timestamp, u64;
//! - with a pack: its block size, u32, its block count, u32, and per block its base, u64, its UID count, u32, and its
//!   group-varint deltas as a u32 length and the bytes, as the encoder writes them;
//! - the posting count, u32, and per posting its uid, u64, op, u32, type, u8, start and commit timestamps, u64 each,
//!   value and language tag as a u32 length and the bytes, facet count, u32, with each facet's key as a u32 length and
//!   its UTF-8 bytes, and statement ID count, u32, with the IDs, u64 each;
//! - the split count, u32, and the splits, u64 each;
//! - the XXH3-64 checksum, seed 0, of every byte before it, u64.
//!
//! A length is a u32 because a stored value is shorter than 2^32 bytes, fjall's bound, so every length inside one
//! fits. Every field is fixed, so a list has one encoding, and a value the decoder accepts encodes back to its bytes.

use ultragraphdb_error_store::StoreError;
use xxhash_rust::xxh3::xxh3_64;

use crate::types::facet::Facet;
use crate::types::posting::Posting;
use crate::types::posting_list::PostingList;
use crate::types::posting_type::PostingType;
use crate::types::uid_block::UidBlock;
use crate::types::uid_pack::UidPack;

/// The version this code writes and reads.
const VERSION: u8 = 1;
/// The flag of a list with a pack.
const HAS_PACK: u8 = 0x01;
/// The bytes of an empty list: version, flags, commit timestamp, posting and split counts, checksum.
const EMPTY_LEN: usize = 1 + 1 + 8 + 4 + 4 + CHECKSUM_LEN;
const CHECKSUM_LEN: usize = 8;
/// A block's fixed fields: base, UID count and the deltas' length.
const BLOCK_LEN: usize = 8 + 4 + 4;
/// A posting's fixed fields: uid, op, type, the two timestamps, and the lengths of its value, language tag, facets
/// and statement IDs.
const POSTING_LEN: usize = 8 + 4 + 1 + 8 + 8 + 4 + 4 + 4 + 4;

impl PostingList {
    /// The list's bytes in the posting format.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(self.encoded_len());
        w.push(VERSION);
        w.push(if self.pack.is_some() { HAS_PACK } else { 0 });
        w.extend_from_slice(&self.commit_ts.to_le_bytes());
        if let Some(pack) = &self.pack {
            w.extend_from_slice(&pack.block_size().to_le_bytes());
            count(&mut w, pack.blocks().len());
            for block in pack.blocks() {
                w.extend_from_slice(&block.base().to_le_bytes());
                w.extend_from_slice(&block.num_uids().to_le_bytes());
                bytes(&mut w, block.deltas());
            }
        }
        count(&mut w, self.postings.len());
        for p in &self.postings {
            w.extend_from_slice(&p.uid().to_le_bytes());
            w.extend_from_slice(&p.op().to_le_bytes());
            w.push(p.posting_type().to_byte());
            w.extend_from_slice(&p.start_ts().to_le_bytes());
            w.extend_from_slice(&p.commit_ts().to_le_bytes());
            bytes(&mut w, p.value());
            bytes(&mut w, p.lang_tag());
            count(&mut w, p.facets().len());
            p.facets()
                .iter()
                .for_each(|f| bytes(&mut w, f.key().as_bytes()));
            count(&mut w, p.statement_ids().len());
            p.statement_ids()
                .iter()
                .for_each(|id| w.extend_from_slice(&id.to_le_bytes()));
        }
        count(&mut w, self.splits.len());
        self.splits
            .iter()
            .for_each(|s| w.extend_from_slice(&s.to_le_bytes()));
        let checksum = xxh3_64(&w);
        w.extend_from_slice(&checksum.to_le_bytes());
        w
    }

    /// The number of bytes [`PostingList::encode`] writes, without writing them.
    pub fn encoded_len(&self) -> usize {
        let pack = self.pack.as_ref().map_or(0, |pack| {
            8 + pack
                .blocks()
                .iter()
                .map(|b| BLOCK_LEN + b.deltas().len())
                .sum::<usize>()
        });
        let postings: usize = self
            .postings
            .iter()
            .map(|p| {
                POSTING_LEN
                    + p.value().len()
                    + p.lang_tag().len()
                    + p.facets().iter().map(|f| 4 + f.key().len()).sum::<usize>()
                    + 8 * p.statement_ids().len()
            })
            .sum();
        EMPTY_LEN + pack + postings + 8 * self.splits.len()
    }

    /// The list stored as `bytes` under `key`.
    ///
    /// # Errors
    ///
    /// The store's `Corrupted`, naming `key`, for bytes of an unknown version, whose checksum fails, whose lengths
    /// overrun them, with unknown flags or posting types, a facet key that is not UTF-8, or bytes after the splits:
    /// nothing is decoded in part.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<PostingList, StoreError> {
        let corrupted = |reason: String| StoreError::Corrupted(key.to_vec(), reason);
        if let Some(&version) = bytes.first()
            && version != VERSION
        {
            return Err(corrupted(format!(
                "unknown posting format version {version}"
            )));
        }
        if bytes.len() < EMPTY_LEN {
            return Err(corrupted(format!(
                "the value is {} bytes, shorter than the {EMPTY_LEN} of an empty posting list",
                bytes.len()
            )));
        }
        let (body, trailer) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
        let stored = u64::from_le_bytes(trailer.try_into().unwrap_or_default());
        let computed = xxh3_64(body);
        if stored != computed {
            return Err(corrupted(format!(
                "checksum {stored:016x} does not match the bytes' {computed:016x}"
            )));
        }

        let mut r = Reader {
            key,
            bytes: body,
            at: 1,
        };
        let flags = r.u8()?;
        if flags & !HAS_PACK != 0 {
            return Err(corrupted(format!("unknown flags {flags:#04x}")));
        }
        let commit_ts = r.u64()?;
        let pack = if flags & HAS_PACK != 0 {
            let block_size = r.u32()?;
            let blocks = (0..r.u32()?)
                .map(|_| {
                    let base = r.u64()?;
                    let num_uids = r.u32()?;
                    Ok(UidBlock::new(base, r.bytes()?, num_uids))
                })
                .collect::<Result<Vec<_>, StoreError>>()?;
            Some(UidPack::new(block_size, blocks))
        } else {
            None
        };
        let postings = (0..r.u32()?)
            .map(|_| r.posting())
            .collect::<Result<Vec<_>, _>>()?;
        let splits = (0..r.u32()?)
            .map(|_| r.u64())
            .collect::<Result<Vec<_>, _>>()?;
        if r.at != body.len() {
            return Err(corrupted(format!(
                "trailing bytes after the splits: {}",
                body.len() - r.at
            )));
        }
        Ok(PostingList {
            pack,
            postings,
            commit_ts,
            splits,
        })
    }
}

/// Writes a count or a length.
fn count(w: &mut Vec<u8>, n: usize) {
    w.extend_from_slice(&(n as u32).to_le_bytes());
}

/// Writes `b` after its length.
fn bytes(w: &mut Vec<u8>, b: &[u8]) {
    count(w, b.len());
    w.extend_from_slice(b);
}

/// Reads a checked body field by field, refusing a field that runs past its end.
struct Reader<'a> {
    key: &'a [u8],
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], StoreError> {
        let field = self
            .at
            .checked_add(n)
            .and_then(|end| self.bytes.get(self.at..end))
            .ok_or_else(|| {
                StoreError::Corrupted(
                    self.key.to_vec(),
                    format!("a field at byte {} overruns the value", self.at),
                )
            })?;
        self.at += n;
        Ok(field)
    }

    fn u8(&mut self) -> Result<u8, StoreError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, StoreError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, StoreError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        let len = self.u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    fn posting(&mut self) -> Result<Posting, StoreError> {
        let uid = self.u64()?;
        let op = self.u32()?;
        let type_at = self.at;
        let byte = self.u8()?;
        let posting_type = PostingType::from_byte(byte).ok_or_else(|| {
            StoreError::Corrupted(
                self.key.to_vec(),
                format!("unknown posting type {byte} at byte {type_at}"),
            )
        })?;
        let start_ts = self.u64()?;
        let commit_ts = self.u64()?;
        let value = self.bytes()?;
        let lang_tag = self.bytes()?;
        let facets = (0..self.u32()?)
            .map(|_| {
                let key_at = self.at;
                String::from_utf8(self.bytes()?)
                    .map(Facet::new)
                    .map_err(|_| {
                        StoreError::Corrupted(
                            self.key.to_vec(),
                            format!("the facet key at byte {key_at} is not UTF-8"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let statement_ids = (0..self.u32()?)
            .map(|_| self.u64())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Posting::default()
            .with_uid(uid)
            .with_op(op)
            .with_posting_type(posting_type)
            .with_start_ts(start_ts)
            .with_commit_ts(commit_ts)
            .with_value(value)
            .with_lang_tag(lang_tag)
            .with_facets(facets)
            .with_statement_ids(statement_ids))
    }
}
