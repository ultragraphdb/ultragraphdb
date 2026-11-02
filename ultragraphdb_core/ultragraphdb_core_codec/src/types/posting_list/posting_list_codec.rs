/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting format: how a posting list is stored. Little-endian, but for every 128-bit value, which is written as
//! its 16 big-endian bytes, the stored form of a system ID:
//!
//! - a version byte, 2;
//! - a flags byte: bit 0 set when the list has a pack, every other bit clear;
//! - the commit timestamp, u64;
//! - with a pack: the pack's bytes as [`UidPack::encode`](crate::UidPack::encode) writes them, its own version byte
//!   first;
//! - the posting count, u32, and per posting its key, 128 bits, op, u32, type, u8, start and commit timestamps, u64
//!   each, value and language tag as a u32 length and the bytes, facet count, u32, with each facet's key as a u32
//!   length and its UTF-8 bytes, and statement ID count, u32, with the IDs, 128 bits each;
//! - the split count, u32, and the keys the parts start at, 128 bits each;
//! - the XXH3-64 checksum, seed 0, of every byte before it, u64.
//!
//! A length is a u32 because a stored value is shorter than 2^32 bytes, fjall's bound, so every length inside one
//! fits. Every field is fixed and the pack has one encoding, so a list has one encoding, and a value the decoder
//! accepts encodes back to its bytes. Stage 5's format, version 1, is refused as an unknown version.

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_error_store::StoreError;
use xxhash_rust::xxh3::xxh3_64;

use crate::types::facet::Facet;
use crate::types::posting::Posting;
use crate::types::posting_list::PostingList;
use crate::types::posting_type::PostingType;
use crate::types::uid_pack::UidPack;

/// The flag of a list with a pack.
const HAS_PACK: u8 = 0x01;
/// The bytes of an empty list: version, flags, commit timestamp, posting and split counts, checksum.
const EMPTY_LEN: usize = 1 + 1 + 8 + 4 + 4 + CHECKSUM_LEN;
const CHECKSUM_LEN: usize = 8;
/// A 128-bit value's bytes.
const U128_LEN: usize = 16;
/// A posting's fixed fields: key, op, type, the two timestamps, and the lengths of its value, language tag, facets
/// and statement IDs.
const POSTING_LEN: usize = U128_LEN + 4 + 1 + 8 + 8 + 4 + 4 + 4 + 4;

const _: () = assert!(EMPTY_LEN == 26 && POSTING_LEN == 53);

impl PostingList {
    /// The version this code writes and reads.
    pub const VERSION: u8 = 2;

    /// The list's bytes in the posting format.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(self.encoded_len());
        self.encode_into(&mut w);
        w
    }

    /// Appends the list's bytes in the posting format to `w`, as [`PostingList::encode`] gives them: what a caller
    /// writing the list into a buffer of its own uses, with no copy between.
    pub fn encode_into(&self, w: &mut Vec<u8>) {
        let start = w.len();
        w.push(Self::VERSION);
        w.push(if self.pack.is_some() { HAS_PACK } else { 0 });
        w.extend_from_slice(&self.commit_ts.to_le_bytes());
        if let Some(pack) = &self.pack {
            pack.encode_into(w);
        }
        count(w, self.postings.len());
        for p in &self.postings {
            w.extend_from_slice(&p.uid().to_be_bytes());
            w.extend_from_slice(&p.op().to_le_bytes());
            w.push(p.posting_type().to_byte());
            w.extend_from_slice(&p.start_ts().to_le_bytes());
            w.extend_from_slice(&p.commit_ts().to_le_bytes());
            bytes(w, p.value());
            bytes(w, p.lang_tag());
            count(w, p.facets().len());
            p.facets().iter().for_each(|f| bytes(w, f.key().as_bytes()));
            count(w, p.statement_ids().len());
            p.statement_ids()
                .iter()
                .for_each(|id| w.extend_from_slice(&id.to_be_bytes()));
        }
        count(w, self.splits.len());
        self.splits
            .iter()
            .for_each(|s| w.extend_from_slice(&s.to_be_bytes()));
        let checksum = xxh3_64(w.get(start..).unwrap_or_default());
        w.extend_from_slice(&checksum.to_le_bytes());
        debug_assert_eq!(
            w.len() - start,
            self.encoded_len(),
            "the length is the bytes'"
        );
    }

    /// The number of bytes [`PostingList::encode`] writes, without writing them.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        let pack = self.pack.as_ref().map_or(0, UidPack::encoded_len);
        let postings: usize = self
            .postings
            .iter()
            .map(|p| {
                POSTING_LEN
                    + p.value().len()
                    + p.lang_tag().len()
                    + p.facets().iter().map(|f| 4 + f.key().len()).sum::<usize>()
                    + U128_LEN * p.statement_ids().len()
            })
            .sum();
        EMPTY_LEN + pack + postings + U128_LEN * self.splits.len()
    }

    /// The list stored as `bytes` under `key`.
    ///
    /// # Errors
    ///
    /// The store's `Corrupted`, naming `key`, for bytes of an unknown version, whose checksum fails, whose lengths
    /// overrun them, with unknown flags or posting types, a pack the pack's decoder refuses, a statement ID that is not
    /// a valid system ID, a facet key that is not UTF-8, or bytes after the splits: nothing is decoded in part.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<PostingList, StoreError> {
        let corrupted = |reason: String| StoreError::corrupted(key.to_vec(), reason);
        if let Some(&version) = bytes.first()
            && version != Self::VERSION
        {
            return Err(corrupted(format!(
                "unknown posting format version {version}"
            )));
        }
        let Some((body, trailer)) = bytes
            .len()
            .checked_sub(CHECKSUM_LEN)
            .filter(|_| bytes.len() >= EMPTY_LEN)
            .and_then(|at| bytes.split_at_checked(at))
        else {
            return Err(corrupted(format!(
                "the value is {} bytes, shorter than the {EMPTY_LEN} of an empty posting list",
                bytes.len()
            )));
        };
        let stored =
            u64::from_le_bytes(<[u8; CHECKSUM_LEN]>::try_from(trailer).unwrap_or_default());
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
        let plist = r.list()?;
        if r.at != body.len() {
            return Err(corrupted(format!(
                "trailing bytes after the splits: {}",
                body.len() - r.at
            )));
        }
        Ok(plist)
    }
}

/// Writes a count or a length; a stored value is shorter than 2^32 bytes, so every count and length fits 32 bits.
fn count(w: &mut Vec<u8>, n: usize) {
    debug_assert!(u32::try_from(n).is_ok(), "a count or length fits 32 bits");
    w.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
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
    fn corrupted(&self, reason: String) -> StoreError {
        StoreError::corrupted(self.key.to_vec(), reason)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], StoreError> {
        let at = self.at;
        let field = at
            .checked_add(n)
            .and_then(|end| self.bytes.get(at..end))
            .ok_or_else(|| self.corrupted(format!("a field at byte {at} overruns the value")))?;
        self.at += n;
        Ok(field)
    }

    /// The next `N` bytes.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], StoreError> {
        let at = self.at;
        let field = self.take(N)?;
        <[u8; N]>::try_from(field)
            .map_err(|_| self.corrupted(format!("a field at byte {at} overruns the value")))
    }

    fn u8(&mut self) -> Result<u8, StoreError> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32, StoreError> {
        Ok(u32::from_le_bytes(self.array::<4>()?))
    }

    fn u64(&mut self) -> Result<u64, StoreError> {
        Ok(u64::from_le_bytes(self.array::<8>()?))
    }

    fn u128(&mut self) -> Result<u128, StoreError> {
        Ok(u128::from_be_bytes(self.array::<U128_LEN>()?))
    }

    /// A count, as many as the bytes left could hold of items of at least `item_len` bytes, for an allocation.
    fn count(&mut self, item_len: usize) -> Result<(usize, usize), StoreError> {
        let n = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        let room = self.bytes.len().saturating_sub(self.at) / item_len.max(1);
        Ok((n, n.min(room)))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        let len = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        Ok(self.take(len)?.to_vec())
    }

    fn list(&mut self) -> Result<PostingList, StoreError> {
        let flags = self.u8()?;
        if flags & !HAS_PACK != 0 {
            return Err(self.corrupted(format!("unknown flags {flags:#04x}")));
        }
        let commit_ts = self.u64()?;
        let pack = if flags & HAS_PACK != 0 {
            let (pack, end) = UidPack::read_at(self.bytes, self.at)
                .map_err(|e| self.corrupted(format!("the pack is corrupt: {e}")))?;
            self.at = end;
            Some(pack)
        } else {
            None
        };
        let (posting_count, posting_room) = self.count(POSTING_LEN)?;
        let mut postings = Vec::with_capacity(posting_room);
        for _ in 0..posting_count {
            postings.push(self.posting()?);
        }
        let (split_count, split_room) = self.count(U128_LEN)?;
        let mut splits = Vec::with_capacity(split_room);
        for _ in 0..split_count {
            splits.push(self.u128()?);
        }
        Ok(PostingList {
            pack,
            postings,
            commit_ts,
            splits,
        })
    }

    fn posting(&mut self) -> Result<Posting, StoreError> {
        let uid = self.u128()?;
        let op = self.u32()?;
        let type_at = self.at;
        let byte = self.u8()?;
        let posting_type = PostingType::from_byte(byte).ok_or_else(|| {
            self.corrupted(format!("unknown posting type {byte} at byte {type_at}"))
        })?;
        let start_ts = self.u64()?;
        let commit_ts = self.u64()?;
        let value = self.bytes()?;
        let lang_tag = self.bytes()?;
        let facets = self.facets()?;
        let statement_ids = self.statement_ids()?;
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

    fn facets(&mut self) -> Result<Vec<Facet>, StoreError> {
        let (n, capacity) = self.count(4)?;
        let mut facets = Vec::with_capacity(capacity);
        for _ in 0..n {
            let key_at = self.at;
            let key = String::from_utf8(self.bytes()?).map_err(|_| {
                self.corrupted(format!("the facet key at byte {key_at} is not UTF-8"))
            })?;
            facets.push(Facet::new(key));
        }
        Ok(facets)
    }

    fn statement_ids(&mut self) -> Result<Vec<SystemId>, StoreError> {
        let (n, capacity) = self.count(U128_LEN)?;
        let mut ids = Vec::with_capacity(capacity);
        for _ in 0..n {
            let at = self.at;
            let id = SystemId::decode(&self.array::<U128_LEN>()?).map_err(|e| {
                self.corrupted(format!("the statement ID at byte {at} is invalid: {e}"))
            })?;
            ids.push(id);
        }
        Ok(ids)
    }
}
