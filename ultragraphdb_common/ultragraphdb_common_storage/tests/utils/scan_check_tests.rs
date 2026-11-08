/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a Scan's ranges may read, checked at its arrival before any frame (`PROTOCOL.md` §13.13), against a core in the test's
//! process.

use std::collections::BTreeSet;

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::{Bucket, Epoch, Sequence, SystemId, TypePrefix, Version};
use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{ErrorCode, Scan, ScanAt, ScanRange, TermTable};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::partition_core::core_harness::{
    Batch, PARTITION, PRINCIPAL, Refused, Setup, TENANT, bind, commit, edges_write, ex,
    forward_key, objects_of, on_core, on_core_in_time, scanned,
};

/// A range: its partition, its first key and its past key.
type Range = (u32, Vec<u8>, Vec<u8>);

/// The frontend's address, which a core of one partition never dials.
pub(crate) fn no_frontend() -> TestResult<Address> {
    Ok(Address::parse("tcp://127.0.0.1:1")?)
}

/// A Scan of no transaction at the partition's latest version, of `ranges`.
fn latest(ranges: &[Range]) -> Scan {
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, [0; 16], 0, 0);
    for (partition, first, past) in ranges {
        scan.push_range(ScanRange::new(*partition, first, past));
    }
    scan
}

/// The first key of `keyspace` in `tenant`: the keyspace's byte, then the tenant's 8 bytes.
fn tenant_first(keyspace: Keyspace, tenant: u64) -> Vec<u8> {
    key_prefix(keyspace, false, tenant, None, None)
}

/// The range of `keyspace` holding all of [`TENANT`].
fn whole(keyspace: Keyspace) -> Range {
    (PARTITION, tenant_first(keyspace, TENANT), Vec::new())
}

/// The range holding `key` alone: its past key is `key` followed by a zero byte.
fn alone(key: Vec<u8>) -> Range {
    let past = [key.as_slice(), &[0]].concat();
    (PARTITION, key, past)
}

/// The ranges `batches` answered whole, each with `end` 1.
fn ended(batches: &[Batch]) -> BTreeSet<u32> {
    batches
        .iter()
        .filter(|batch| batch.end == 1)
        .flat_map(|batch| batch.range..=batch.through)
        .collect()
}

/// The refusal `scan` takes before any frame.
async fn refusal(client: &mut RoleClient, scan: &Scan) -> TestResult<Refused> {
    let (batches, refused) = scanned(client, scan).await?;
    assert!(batches.is_empty(), "no frame before a refusal: {batches:?}");
    refused.context("a refusal")
}

/// The keys `scan`'s answer holds, in order, the Scan answered whole.
async fn keys_read(client: &mut RoleClient, scan: &Scan) -> TestResult<Vec<Vec<u8>>> {
    let (batches, refused) = scanned(client, scan).await?;
    assert!(refused.is_none(), "{refused:?}");
    Ok(batches
        .iter()
        .flat_map(|batch| batch.entries.iter().map(|(key, _)| key.clone()))
        .collect())
}

/// The value of the first entry `scan`'s answer holds, the Scan answered whole.
async fn first_value(client: &mut RoleClient, scan: &Scan) -> TestResult<Vec<u8>> {
    let (batches, refused) = scanned(client, scan).await?;
    assert!(refused.is_none(), "{refused:?}");
    let (_, value) = batches
        .first()
        .and_then(|batch| batch.entries.first())
        .context("an entry")?;
    Ok(value.clone())
}

/// That `refused` is Error 1 (Protocol), its message holding `what`.
fn protocol_naming(refused: &Refused, what: &str) {
    assert_eq!(refused.code, ErrorCode::Protocol, "{refused:?}");
    assert!(refused.message.contains(what), "{refused:?} names {what}");
}

/// A range of each keyspace a Scan reads answers, every range of one tenant, a past key within the tenant or at the first key past
/// it as well as at its end.
#[test]
fn test_a_range_of_each_keyspace_a_scan_reads_answers() -> TestResult {
    on_core_in_time(
        "scan-keyspaces",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let first = tenant_first(Keyspace::Forward, TENANT);
            let within = [first.as_slice(), &[0xFF]].concat();
            let past_tenant = tenant_first(Keyspace::Forward, TENANT + 1);
            let ranges = [
                whole(Keyspace::Forward),
                (PARTITION, first.clone(), within),
                (PARTITION, first, past_tenant),
                whole(Keyspace::Statements),
                whole(Keyspace::Referrers),
                alone(term_key(TENANT, &ex("a"))),
            ];
            let (batches, refused) = scanned(&mut client, &latest(&ranges)).await?;
            assert!(refused.is_none(), "{refused:?}");
            let expected: BTreeSet<u32> = (0..6).collect();
            assert_eq!(ended(&batches), expected);
            Ok(())
        },
    )
}

/// A range of a keyspace a Scan does not read is refused, the message naming the keyspace.
#[test]
fn test_a_range_of_a_keyspace_no_scan_reads_is_refused() -> TestResult {
    on_core(
        "scan-no-keyspace",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            for keyspace in [Keyspace::Statistics, Keyspace::Catalog, Keyspace::Marks] {
                let refused = refusal(&mut client, &latest(&[whole(keyspace)])).await?;
                protocol_naming(
                    &refused,
                    &format!("reads no range of the keyspace {keyspace:?}"),
                );
            }
            Ok(())
        },
    )
}

/// A range of a partition the address does not serve is refused, naming it.
#[test]
fn test_a_range_of_another_partition_is_refused() -> TestResult {
    on_core(
        "scan-partition",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let (_, first, past) = whole(Keyspace::Forward);
            let refused = refusal(&mut client, &latest(&[(PARTITION + 1, first, past)])).await?;
            protocol_naming(
                &refused,
                &format!("hosts partition {PARTITION}, not {}", PARTITION + 1),
            );
            Ok(())
        },
    )
}

/// A Scan's ranges name one tenant: a second naming another is refused, naming both.
#[test]
fn test_ranges_of_two_tenants_are_refused() -> TestResult {
    on_core(
        "scan-tenants",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let other = (PARTITION, tenant_first(Keyspace::Forward, 3), Vec::new());
            let refused = refusal(&mut client, &latest(&[whole(Keyspace::Forward), other])).await?;
            protocol_naming(
                &refused,
                &format!("range 1 reads tenant 3, range 0 tenant {TENANT}"),
            );
            Ok(())
        },
    )
}

/// A range of the dictionary holds one key: a wider one is refused, as is a past key above the first key past its tenant.
#[test]
fn test_a_range_wider_than_it_may_read_is_refused() -> TestResult {
    on_core(
        "scan-wide",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let key = term_key(TENANT, &ex("a"));
            let wide = [key.as_slice(), &[1]].concat();
            let wider = refusal(&mut client, &latest(&[(PARTITION, key, wide)])).await?;
            protocol_naming(&wider, "range 0 of the dictionary holds more than one key");
            let beyond = [tenant_first(Keyspace::Forward, TENANT + 1).as_slice(), &[0]].concat();
            let ranges = [(PARTITION, tenant_first(Keyspace::Forward, TENANT), beyond)];
            let past_tenant = refusal(&mut client, &latest(&ranges)).await?;
            protocol_naming(&past_tenant, "range 0: its past key lies beyond its tenant");
            Ok(())
        },
    )
}

/// A range reads up to its past key and no further: a list range stops below it, and a range of the dictionary holds its one key.
#[test]
fn test_a_past_key_bounds_what_a_range_reads() -> TestResult {
    on_core(
        "scan-past",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let _version = commit(&mut client, &edges_write(&[("a", "b"), ("c", "d")])).await?;
            let mut keys = [
                forward_key(&mut client, "a").await?,
                forward_key(&mut client, "c").await?,
            ];
            keys.sort();
            let [low, high] = keys;
            let ranges = [(PARTITION, tenant_first(Keyspace::Forward, TENANT), high)];
            assert_eq!(keys_read(&mut client, &latest(&ranges)).await?, [low]);
            let term = term_key(TENANT, &ex("a"));
            let one = latest(&[alone(term.clone())]);
            assert_eq!(keys_read(&mut client, &one).await?, [term]);
            Ok(())
        },
    )
}

/// A list range with an `after` reads its list from that object on, the object included; an `after` in a keyspace of no lists is
/// refused, naming the range.
#[test]
fn test_an_after_resumes_a_list_at_its_object() -> TestResult {
    on_core(
        "scan-after",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let _version = commit(&mut client, &edges_write(&[("a", "b"), ("a", "c")])).await?;
            let key = forward_key(&mut client, "a").await?;
            let objects =
                objects_of(&first_value(&mut client, &latest(&[alone(key.clone())])).await?)?;
            assert_eq!(objects.len(), 2, "the list holds both objects");
            let last = objects.last().context("its last object")?;
            let past_key = [key.as_slice(), &[0]].concat();
            let mut resumed = latest(&[]);
            resumed.push_range(ScanRange::new(PARTITION, &key, &past_key).with_after(last));
            let resumed_objects = objects_of(&first_value(&mut client, &resumed).await?)?;
            assert_eq!(resumed_objects.as_slice(), std::slice::from_ref(last));
            let (_, first, past) = whole(Keyspace::Statements);
            let mut statements = latest(&[]);
            statements.push_range(ScanRange::new(PARTITION, &first, &past).with_after(last));
            let refused = refusal(&mut client, &statements).await?;
            protocol_naming(
                &refused,
                "range 0 holds an `after`, which only a keyspace of lists has",
            );
            Ok(())
        },
    )
}

/// A Scan of the transaction `identity` at `read_version` as `principal`, naming the IRIs `terms`, of `ranges`.
fn of_transaction(
    identity: u8,
    read_version: u64,
    principal: u64,
    terms: &[&str],
    ranges: &[Range],
) -> Scan {
    let mut table = TermTable::new();
    for term in terms {
        let _index = table.iri(term);
    }
    let mut scan = Scan::new(
        ScanAt::ReadVersion,
        read_version,
        0,
        principal,
        [identity; 16],
        0,
        0,
    )
    .with_terms(table);
    for (partition, first, past) in ranges {
        scan.push_range(ScanRange::new(*partition, first, past));
    }
    scan
}

/// A Scan of a transaction at its version and principal, its first range holding its term's dictionary key alone, answers that
/// key's ID.
#[test]
fn test_a_scan_of_its_transaction_answers() -> TestResult {
    on_core(
        "scan-bound",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let (_, read_version) = bind(&mut client, 9).await?;
            let iri = ex("a");
            let key = term_key(TENANT, &iri);
            let scan = of_transaction(9, read_version, PRINCIPAL, &[&iri], &[alone(key.clone())]);
            assert_eq!(
                keys_read(&mut client, &scan).await?,
                [key],
                "the term's ID, got or created"
            );
            Ok(())
        },
    )
}

/// A Scan of a transaction at another version, as another principal, of a referrer set, or whose first range does not hold its
/// term's dictionary key, is refused, naming what differs.
#[test]
fn test_a_scan_not_of_its_transaction_is_refused() -> TestResult {
    on_core(
        "scan-not-bound",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let (_, read_version) = bind(&mut client, 9).await?;
            let iri = ex("a");
            let key = alone(term_key(TENANT, &iri));
            // A version bound no Bind of a partition that has closed nothing answers.
            let other = Version::bound_of(Epoch::new(5)?);
            assert_ne!(other, read_version);
            let cases = [
                (
                    of_transaction(9, other, PRINCIPAL, &[&iri], std::slice::from_ref(&key)),
                    format!("read_version {other} is not the transaction's"),
                ),
                (
                    of_transaction(9, read_version, PRINCIPAL + 1, &[&iri], &[key]),
                    format!("principal {} is not the transaction's", PRINCIPAL + 1),
                ),
                (
                    of_transaction(
                        9,
                        read_version,
                        PRINCIPAL,
                        &[],
                        &[whole(Keyspace::Referrers)],
                    ),
                    "range 0 reads a referrer set".to_string(),
                ),
                (
                    of_transaction(
                        9,
                        read_version,
                        PRINCIPAL,
                        &[&iri],
                        &[alone(term_key(TENANT, &ex("b")))],
                    ),
                    "range 0 does not hold the dictionary key of scan term 0".to_string(),
                ),
            ];
            for (scan, what) in &cases {
                protocol_naming(&refusal(&mut client, scan).await?, what);
            }
            Ok(())
        },
    )
}

/// A range whose first key names no tenant, a split part, a byte no keyspace has, or an `after` a range of it cannot take, an
/// ID's tag holding no valid system ID among them, is refused before any frame, naming what is wrong.
#[test]
fn test_a_range_naming_no_tenant_or_keyspace_or_a_wrong_after_is_refused() -> TestResult {
    on_core(
        "scan-malformed",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let short = vec![Keyspace::Forward.byte(), 1, 2];
            let split = key_prefix(Keyspace::Forward, true, TENANT, None, None);
            let unknown = [vec![0xEE], TENANT.to_be_bytes().to_vec()].concat();
            for (range, named) in [
                (
                    (PARTITION, short, Vec::new()),
                    "too short to name a tenant".to_string(),
                ),
                (
                    (PARTITION, split, Vec::new()),
                    format!("no split part of the keyspace {:?}", Keyspace::Forward),
                ),
                (
                    (PARTITION, unknown, Vec::new()),
                    "reads no range of the keyspace 0xee".to_string(),
                ),
            ] {
                let refused = refusal(&mut client, &latest(&[range])).await?;
                protocol_naming(&refused, &named);
            }
            // An ID's tag and 16 bytes whose last reserved bit is set, which name no system ID.
            let id = EncodedTerm::id(SystemId::new(
                TypePrefix::RESOURCE,
                Bucket::new(1)?,
                Sequence::new(3)?,
            ));
            let no_id = [&id.as_bytes()[..SystemId::BYTES], &[1][..]].concat();
            for (keyspace, after, named) in [
                (
                    Keyspace::Statements,
                    &b"x"[..],
                    "only a keyspace of lists has",
                ),
                (
                    Keyspace::Forward,
                    &[0xFF][..],
                    "not one whole object encoding",
                ),
                (
                    Keyspace::Forward,
                    &no_id[..],
                    "not one whole object encoding",
                ),
            ] {
                let (_, first, past) = whole(keyspace);
                let mut scan = latest(&[]);
                scan.push_range(ScanRange::new(PARTITION, &first, &past).with_after(after));
                let refused = refusal(&mut client, &scan).await?;
                protocol_naming(&refused, named);
            }
            Ok(())
        },
    )
}

/// A Scan of a transaction reading another tenant's range is refused naming both tenants.
#[test]
fn test_a_scan_of_its_transaction_reads_its_tenant_alone() -> TestResult {
    on_core(
        "scan-bound-tenant",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let (_, read_version) = bind(&mut client, 9).await?;
            let other = TENANT + 1;
            let iri = ex("a");
            let elsewhere = of_transaction(
                9,
                read_version,
                PRINCIPAL,
                &[&iri],
                &[alone(term_key(other, &iri))],
            );
            protocol_naming(
                &refusal(&mut client, &elsewhere).await?,
                &format!("range 0 reads tenant {other}, the transaction's being {TENANT}"),
            );
            Ok(())
        },
    )
}
