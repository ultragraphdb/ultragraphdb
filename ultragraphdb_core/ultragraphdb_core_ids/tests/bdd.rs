/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system-identity scenarios, bound to `features/system-identity/`.

use cucumber::{World, then, when};
use serde::Deserialize;
use ultragraphdb_core_ids::{
    Bucket, CanonicalKey, IdError, IdErrorEnum, Namespace, Sequence, SystemId, TypePrefix,
};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

const IRI: &str = "http://ex.org/a";

#[derive(Debug, Default, World)]
struct Steps {
    ids: Vec<SystemId>,
    sorted: Vec<[u8; 16]>,
    refusals: Vec<Result<SystemId, IdError>>,
    buckets: Vec<Bucket>,
    encoded: Vec<u8>,
}

/// A golden case of `testdata/golden/buckets.json`, written by `buckets.py` from the xxHash reference.
#[derive(Debug, Deserialize)]
struct Case {
    namespace: u64,
    prefix: u16,
    iri: String,
    namespace_xxh3: String,
    key_xxh3: String,
    bucket: u32,
}

/// The reference's two hashes and the bucket for `iri` of the Resource type in `namespace`.
fn reference(namespace: u64, iri: &str) -> TestResult<(u64, u64, u32)> {
    let golden = load_golden::<Case>(&testdata_dir()?.join("golden").join("buckets.json"))?;
    let case = golden
        .cases()
        .iter()
        .find(|c| c.namespace == namespace && c.prefix == 1 && c.iri == iri)
        .context("the golden case")?;
    Ok((
        u64::from_str_radix(&case.namespace_xxh3, 16).context("a hex hash")?,
        u64::from_str_radix(&case.key_xxh3, 16).context("a hex hash")?,
        case.bucket,
    ))
}

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

fn bucket_of(namespace: u64, iri: &str) -> TestResult<Bucket> {
    Ok(Bucket::derive(
        Namespace::new(namespace),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(iri)?,
    ))
}

#[when("an ID is built from type prefix 3, bucket 0xAB_CDEF and sequence 42")]
fn an_id_is_built(w: &mut Steps) -> TestResult {
    w.ids.push(id(3, 0xAB_CDEF, 42)?);
    Ok(())
}

#[then(
    "its 16 big-endian bytes are `00 03`, `AB CD EF`, `00 00 00 00 00 00 00 2A` and `00 00 00`, and unpacking returns type 3, bucket 0xAB_CDEF and sequence 42"
)]
fn its_bytes_and_fields(w: &mut Steps) -> TestResult {
    let built = *w.ids.first().context("the built ID")?;
    let bytes = [
        0x00, 0x03, 0xAB, 0xCD, 0xEF, 0, 0, 0, 0, 0, 0, 0, 0x2A, 0, 0, 0,
    ];
    assert_eq!(built.to_be_bytes(), bytes);
    let unpacked = SystemId::decode(&bytes)?;
    assert_eq!(unpacked.type_prefix().get(), 3);
    assert_eq!(unpacked.bucket().get(), 0xAB_CDEF);
    assert_eq!(unpacked.sequence().get(), 42);
    Ok(())
}

#[when("16 bytes whose last byte is 0x01 are decoded as an ID")]
fn reserved_byte_decoded(w: &mut Steps) -> TestResult {
    // A valid ID's bytes with the last byte 0x01, and all zero bytes but the last.
    let mut bytes = id(3, 0xAB_CDEF, 42)?.to_be_bytes();
    bytes[15] = 0x01;
    w.refusals.push(SystemId::decode(&bytes));
    let mut zeros = [0u8; 16];
    zeros[15] = 0x01;
    w.refusals.push(SystemId::decode(&zeros));
    Ok(())
}

#[then("decoding fails with an invalid-ID error naming the reserved bits")]
fn refused_naming_reserved_bits(w: &mut Steps) -> TestResult {
    assert_eq!(w.refusals.len(), 2);
    for refusal in &w.refusals {
        let error = refusal.err().context("a refusal")?;
        assert_eq!(error.0, IdErrorEnum::ReservedBits { bits: 1 });
        assert!(error.to_string().contains("reserved bits"), "{error}");
    }
    Ok(())
}

#[when("an ID with type prefix 0, and another with sequence 0, are decoded")]
fn zero_fields_decoded(w: &mut Steps) -> TestResult {
    let mut zero_prefix = id(3, 0xAB_CDEF, 42)?.to_be_bytes();
    zero_prefix[1] = 0;
    let mut zero_sequence = id(3, 0xAB_CDEF, 42)?.to_be_bytes();
    zero_sequence[12] = 0;
    w.refusals.push(SystemId::decode(&zero_prefix));
    w.refusals.push(SystemId::decode(&zero_sequence));
    Ok(())
}

#[then("each fails with an invalid-ID error naming the field")]
fn each_refused_naming_the_field(w: &mut Steps) -> TestResult {
    let prefix = w
        .refusals
        .first()
        .context("two decodes")?
        .err()
        .context("the zero prefix refused")?;
    let sequence = w
        .refusals
        .get(1)
        .context("two decodes")?
        .err()
        .context("the zero sequence refused")?;
    assert_eq!(prefix.0, IdErrorEnum::ZeroTypePrefix);
    assert_eq!(sequence.0, IdErrorEnum::ZeroSequence);
    assert!(prefix.to_string().contains("type prefix"), "{prefix}");
    assert!(sequence.to_string().contains("sequence"), "{sequence}");
    Ok(())
}

#[when(
    "the IDs (type 2, bucket 9, sequence 1), (type 1, bucket 900, sequence 7) and (type 1, bucket 3, sequence 99) are sorted by their bytes"
)]
fn ids_sorted_by_bytes(w: &mut Steps) -> TestResult {
    w.sorted = vec![
        id(2, 9, 1)?.to_be_bytes(),
        id(1, 900, 7)?.to_be_bytes(),
        id(1, 3, 99)?.to_be_bytes(),
    ];
    w.sorted.sort();
    Ok(())
}

#[then("the order is (1, 3, 99), (1, 900, 7), (2, 9, 1)")]
fn the_sorted_order(w: &mut Steps) -> TestResult {
    let fields = w
        .sorted
        .iter()
        .map(|bytes| {
            let decoded = SystemId::decode(bytes)?;
            Ok((
                decoded.type_prefix().get(),
                decoded.bucket().get(),
                decoded.sequence().get(),
            ))
        })
        .collect::<TestResult<Vec<_>>>()?;
    assert_eq!(fields, [(1, 3, 99), (1, 900, 7), (2, 9, 1)]);
    Ok(())
}

#[when("the bucket of the IRI `http://ex.org/a` in namespace 0 is computed")]
fn bucket_in_namespace_zero(w: &mut Steps) -> TestResult {
    w.buckets.push(bucket_of(0, IRI)?);
    Ok(())
}

#[then(
    "its top 8 bits equal the top byte of the xxHash reference implementation's XXH3-64 of eight zero bytes, and its low 16 bits equal the top two bytes of the reference XXH3-64 of the `Resource` prefix's 2 bytes followed by the IRI's canonical encoding, both values committed as golden values with their source"
)]
fn bucket_matches_reference(w: &mut Steps) -> TestResult {
    let bucket = w.buckets.first().context("the bucket")?.get();
    let (namespace_hash, key_hash, golden_bucket) = reference(0, IRI)?;
    assert_eq!(u64::from(bucket >> 16), namespace_hash >> 56);
    assert_eq!(u64::from(bucket & 0xFFFF), key_hash >> 48);
    assert_eq!(bucket, golden_bucket);
    Ok(())
}

#[when("a statement with subject `http://ex.org/a` receives a system ID")]
fn statement_receives_id(w: &mut Steps) -> TestResult {
    let subject = SystemId::resource(
        Namespace::new(0),
        &CanonicalKey::iri(IRI)?,
        Sequence::new(11)?,
    );
    w.ids.push(SystemId::statement(subject, Sequence::new(12)?));
    Ok(())
}

#[then("the statement's bucket equals the bucket of `http://ex.org/a`")]
fn statement_bucket_is_subjects(w: &mut Steps) -> TestResult {
    let statement = *w.ids.first().context("the statement's ID")?;
    let (_, _, golden_bucket) = reference(0, IRI)?;
    assert_eq!(statement.bucket().get(), golden_bucket);
    assert_eq!(statement.bucket(), bucket_of(0, IRI)?);
    Ok(())
}

#[when("the IRI `http://ex.org/a` is bucketed in namespaces 0 and 1")]
fn bucketed_in_two_namespaces(w: &mut Steps) -> TestResult {
    w.buckets.push(bucket_of(0, IRI)?);
    w.buckets.push(bucket_of(1, IRI)?);
    Ok(())
}

#[then(
    "the two buckets' low 16 bits are equal, and their top 8 bits are the reference values for each namespace"
)]
fn low_bits_equal_top_bits_reference(w: &mut Steps) -> TestResult {
    let zero = w.buckets.first().context("two buckets")?.get();
    let one = w.buckets.get(1).context("two buckets")?.get();
    assert_eq!(zero & 0xFFFF, one & 0xFFFF);
    let (hash_zero, _, _) = reference(0, IRI)?;
    let (hash_one, _, _) = reference(1, IRI)?;
    assert_eq!(u64::from(zero >> 16), hash_zero >> 56);
    assert_eq!(u64::from(one >> 16), hash_one >> 56);
    assert_ne!(zero >> 16, one >> 16);
    Ok(())
}

#[when("the IRI `http://ex.org/a` is canonically encoded")]
fn canonically_encoded(w: &mut Steps) -> TestResult {
    CanonicalKey::iri(IRI)?.encode_into(&mut w.encoded);
    Ok(())
}

#[then("the bytes are `00 00 00 0F` followed by the 15 UTF-8 bytes of `http://ex.org/a`")]
fn the_canonical_bytes(w: &mut Steps) -> TestResult {
    let (length, iri) = w.encoded.split_first_chunk::<4>().context("a length")?;
    assert_eq!(length, &[0x00, 0x00, 0x00, 0x0F]);
    assert_eq!(iri, b"http://ex.org/a");
    assert_eq!(iri.len(), 15);
    Ok(())
}

#[when("a new IRI and a new statement receive IDs")]
fn new_iri_and_statement(w: &mut Steps) -> TestResult {
    let iri = SystemId::resource(
        Namespace::new(3),
        &CanonicalKey::iri(IRI)?,
        Sequence::new(1)?,
    );
    w.ids.push(iri);
    w.ids.push(SystemId::statement(iri, Sequence::new(1)?));
    Ok(())
}

#[then("the IRI's type prefix is 1 and the statement's is 2")]
fn reserved_prefixes(w: &mut Steps) -> TestResult {
    let iri = *w.ids.first().context("the IRI's ID")?;
    let statement = *w.ids.get(1).context("the statement's ID")?;
    assert_eq!(iri.type_prefix().get(), 1);
    assert_eq!(statement.type_prefix().get(), 2);
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
