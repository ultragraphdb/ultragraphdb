/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a Scan's ranges read beside the lists and what a Bind stream takes, through real frames: a statement's record, a run of
//! dictionary lookups, a list resumed at an `after`, Held and the refusals of a stream no Bind opened, and a home's refusals.

use std::path::Path;
use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::{Bucket, Epoch, SystemId, Version};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::Names;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Abort, Bind, BindKind, BindView, BodyView, Cancel, Command, Commit, CommitOperation,
    DropCommand, EntriesEnd, EntriesSequence, ErrorCode, ErrorReplyView, FLAG_LAST,
    MAX_FRAME_FLOOR, ObjectRecords, Operation, Reference, ReferenceView, RequestTerm, Scan, ScanAt,
    ScanRange, StatementTarget, TermTable, Write, WriteChange,
};
use ultragraphdb_store_kv::DATA_KEYSPACE;
use ultragraphdb_store_quads::{StatementRecord, id_key, iri_bucket, tenant_base, term_key};
use ultragraphdb_tests::{CLUSTER, SECRET};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::{stored, write_raw_at};

use crate::played_home::{HomeScript, play_home, played_id};
use crate::read_kernel_steps::{
    Frame, NO_TRANSACTION, PARTITION, PRINCIPAL, Refused, address, alone, answer, bind_first,
    bind_of, closed_by, commit_answer, dial, dial_at, forward_key, homed_at_three, id_of,
    next_frame, of_transaction, one_partition, plain, protocol_naming, refused_scan, run,
    term_scan, write_held,
};
use crate::{PREDICATE, Steps, TENANT, commit_at, edge_write, subject_of, two_processes};

/// How long a test watches a connection stay silent.
const SILENCE: Duration = Duration::from_millis(50);

/// The predicate a tenant's initial catalog names.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The statement of `<a> <p> <b>` these scenarios read.
const SUBJECT: &str = "http://ex.org/a";
const OBJECT: &str = "http://ex.org/b";
const SINCE: &str = "http://ex.org/since";
const UNTIL: &str = "http://ex.org/until";

/// An edge of [`SUBJECT`] to [`OBJECT`] with `properties`, each a predicate and a simple literal.
fn statement_write(properties: &[(&str, &str)]) -> TestResult<Write> {
    let mut table = TermTable::new();
    let subject = table.iri(SUBJECT);
    let predicate = table.iri(PREDICATE);
    let object = table.iri(OBJECT);
    let properties = properties
        .iter()
        .map(|(key, value)| {
            let key = table.iri(key);
            let value = table.intern(RequestTerm::Literal {
                lexical: (*value).to_string(),
                datatype: None,
            })?;
            Ok((key, value))
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(Write::new(
        table,
        vec![WriteChange::InsertEdge {
            graph: None,
            subject,
            predicate,
            object,
            properties,
        }],
    ))
}

/// The statement ID of the one object the list at `client` of [`SUBJECT`] holds under one.
async fn statement_of(client: &mut RoleClient) -> TestResult<SystemId> {
    let key = forward_key(client, SUBJECT).await?;
    let (batches, refused) = answer(
        client,
        &plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]),
        false,
    )
    .await?;
    assert!(refused.is_none(), "{refused:?}");
    let (_, value) = batches
        .first()
        .and_then(|batch| batch.entries.first())
        .context("the list")?;
    let records = ObjectRecords::read(value)?;
    let id = records
        .iter()
        .find_map(|record| record.statement_ids().next())
        .context("a statement ID")?;
    Ok(SystemId::decode(&id)?)
}

fn record_key(statement: SystemId) -> Vec<u8> {
    QuadKey::Statement {
        tenant: TENANT,
        graph: Graph::Default,
        statement,
    }
    .to_bytes()
}

/// The first value `batches`' first entry holds.
fn entry_of(batches: &[crate::read_kernel_steps::Batch]) -> TestResult<(Vec<u8>, Vec<u8>)> {
    batches
        .first()
        .and_then(|batch| batch.entries.first())
        .cloned()
        .context("the entry")
}

/// The record of `(key, value)`'s values of `property`, by whether each holds `text` inline.
fn record_holds(key: &[u8], value: &[u8], property: SystemId, text: &str) -> TestResult<bool> {
    let record = StatementRecord::decode(key, value)?;
    Ok(record
        .properties()
        .iter()
        .filter(|(held, _)| *held == property)
        .any(|(_, held)| {
            held.as_bytes()
                .windows(text.len())
                .any(|part| part == text.as_bytes())
        }))
}

#[when("compute scans the key of a statement's record at its subject's partition, at a version")]
fn record_scanned(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let first = commit_at(&address(), PARTITION, &statement_write(&[(SINCE, "2019")])?).await?;
        let mut client = dial().await?;
        let statement = statement_of(&mut client).await?;
        let key = record_key(statement);
        let _later = commit_at(
            &address(),
            PARTITION,
            &set_since(statement, &mut client).await?,
        )
        .await?;
        let range = alone(key.clone());
        let then = plain(ScanAt::Named, first, 0, 0, std::slice::from_ref(&range));
        let (older, then_refused) = answer(&mut client, &then, false).await?;
        assert!(then_refused.is_none(), "{then_refused:?}");
        let now = plain(ScanAt::Latest, 0, 0, 0, std::slice::from_ref(&range));
        let (latest, now_refused) = answer(&mut client, &now, false).await?;
        assert!(now_refused.is_none(), "{now_refused:?}");
        let since = id_of(&mut client, SINCE).await?;
        w.kernel.bytes = vec![key, since.to_be_bytes().to_vec()];
        w.kernel.batches = older;
        w.kernel.batches.extend(latest);
        Ok(())
    })
}

/// The Write setting `<since> 2020` through the reifier of `statement`.
async fn set_since(statement: SystemId, client: &mut RoleClient) -> TestResult<Write> {
    let reifier = reifier_of(client, statement).await?;
    let mut table = TermTable::new();
    let target = StatementTarget::Reifier {
        graph: None,
        reifier: table.iri(&reifier),
    };
    let predicate = table.iri(SINCE);
    let value = table.intern(RequestTerm::Literal {
        lexical: "2020".to_string(),
        datatype: None,
    })?;
    Ok(Write::new(
        table,
        vec![WriteChange::SetProperty {
            statement: target,
            predicate,
            value,
        }],
    ))
}

/// The skolem IRI naming `statement`, by its entry in the dictionary.
async fn reifier_of(client: &mut RoleClient, statement: SystemId) -> TestResult<String> {
    let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(id_key(TENANT, statement))]);
    let (batches, _) = answer(client, &scan, false).await?;
    Ok(String::from_utf8(entry_of(&batches)?.1)?)
}

#[then("the entry's value is the record's stored bytes at that version")]
fn record_at_version(w: &mut Steps) -> TestResult {
    let [key, since] = w.kernel.bytes.as_slice() else {
        return Err(TestError::new("the key and the property"));
    };
    let since = SystemId::decode(since)?;
    let [older, latest] = w.kernel.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.batches)));
    };
    let (held, then) = older.entries.first().context("the older record")?;
    assert_eq!(held, key);
    assert!(record_holds(key, then, since, "2019")?);
    assert!(!record_holds(key, then, since, "2020")?, "at that version");
    let (_, now) = latest.entries.first().context("the latest record")?;
    assert!(record_holds(key, now, since, "2020")?);
    assert_eq!(older.end, EntriesEnd::Ended.number());
    w.roles = None;
    Ok(())
}

#[when(
    "a transaction sets `<since> 2020` through the reifier `r`, the write is answered, and a Scan naming the transaction reads the statement's record"
)]
fn own_property(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let held = statement_write(&[(SINCE, "2019"), (UNTIL, "2030")])?;
        let _held = commit_at(&address(), PARTITION, &held).await?;
        let mut client = dial().await?;
        let statement = statement_of(&mut client).await?;
        let key = record_key(statement);
        let setting = set_since(statement, &mut client).await?;
        let removing = remove_until(&mut client, statement).await?;
        let reply = client.request(&bind_of(40, PRINCIPAL)).await?;
        let read = BindView::from_frame(&reply.view()?)?.read_version();
        drop(reply);
        let stream = client.last_stream();
        write_held(&mut client, stream, &setting).await?;
        write_held(&mut client, stream, &removing).await?;
        let mut scan = of_transaction(40, read, PRINCIPAL);
        let (first, past) = alone(key.clone());
        scan.push_range(ScanRange::new(PARTITION, &first, &past));
        let (own, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        let plain_scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(key.clone())]);
        let (seen, _) = answer(&mut dial().await?, &plain_scan, false).await?;
        let (since, until) = (
            id_of(&mut client, SINCE).await?,
            id_of(&mut client, UNTIL).await?,
        );
        // A property of the same record written above the read version conflicts with what the span covered.
        let again = set_since(statement, &mut dial().await?).await?;
        let _second = commit_at(&address(), PARTITION, &again).await?;
        w.kernel
            .refused
            .extend(commit_answer(&mut client, stream).await?);
        w.kernel.bytes = vec![since.to_be_bytes().to_vec(), until.to_be_bytes().to_vec()];
        w.kernel.batches = own;
        w.kernel.batches.extend(seen);
        Ok(())
    })
}

/// The Write removing `<until>` through the reifier of `statement`.
async fn remove_until(client: &mut RoleClient, statement: SystemId) -> TestResult<Write> {
    let reifier = reifier_of(client, statement).await?;
    let mut table = TermTable::new();
    let target = StatementTarget::Reifier {
        graph: None,
        reifier: table.iri(&reifier),
    };
    let predicate = table.iri(UNTIL);
    Ok(Write::new(
        table,
        vec![WriteChange::DeleteProperty {
            statement: target,
            predicate,
            value: None,
        }],
    ))
}

#[then(
    "the record holds `<since> 2020` over what the Bind's version holds, a property the transaction removed is absent from it, and its span is recorded as a range scanned"
)]
fn own_property_seen(w: &mut Steps) -> TestResult {
    let [since, until] = w.kernel.bytes.as_slice() else {
        return Err(TestError::new("the properties"));
    };
    let (since, until) = (SystemId::decode(since)?, SystemId::decode(until)?);
    let [own, seen] = w.kernel.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.batches)));
    };
    let (key, value) = own.entries.first().context("its record")?;
    assert!(record_holds(key, value, since, "2020")?);
    assert!(!record_holds(key, value, until, "2030")?, "it removed it");
    let (_, others) = seen.entries.first().context("the store's record")?;
    assert!(record_holds(key, others, since, "2019")?, "nobody else's");
    assert!(record_holds(key, others, until, "2030")?);
    let refused = w.kernel.refused.first().context("the abort")?;
    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(
        refused.detail,
        ultragraphdb_protocol_wire::ConflictKind::Range.number()
    );
    w.roles = None;
    Ok(())
}

#[when("a Scan's second range holds a first key of another tenant than its first range's")]
fn two_tenants(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let held = key_prefix(Keyspace::Forward, false, TENANT, None, None);
    let other = key_prefix(Keyspace::Forward, false, TENANT + 1, None, None);
    let ranges = [(held, Vec::new()), (other, Vec::new())];
    let refused = run(async { refused_scan(&mut dial().await?, &ranges).await })?;
    w.kernel.refused = vec![refused];
    Ok(())
}

#[then(
    "it is refused with Error 1, the message naming the range, and no frame precedes the refusal"
)]
fn second_range_named(w: &mut Steps) -> TestResult {
    protocol_naming(w.kernel.refused.first().context("the refusal")?, "range 1");
    w.roles = None;
    Ok(())
}

/// The lookups of the run's scenario.
const LOOKUPS: u32 = 50;

/// Which of them no dictionary holds.
const ABSENT: [u32; 3] = [7, 23, 41];

fn lookup(at: u32) -> String {
    format!("http://ex.org/t{at}")
}

#[when("a Scan holds 50 one-key ranges of the dictionary, 3 of whose keys are absent")]
fn run_of_lookups(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let held: Vec<String> = (0..LOOKUPS)
        .filter(|at| !ABSENT.contains(at))
        .map(lookup)
        .collect();
    let mut table = TermTable::new();
    let predicate = table.iri(PREDICATE);
    let object = table.iri(OBJECT);
    let changes = held
        .iter()
        .map(|subject| WriteChange::Insert {
            graph: None,
            subject: table.iri(subject),
            predicate,
            object,
        })
        .collect();
    let write = Write::new(table, changes);
    let ranges: Vec<_> = (0..LOOKUPS)
        .map(|at| alone(term_key(TENANT, &lookup(at))))
        .collect();
    run(async {
        let _version = commit_at(&address(), PARTITION, &write).await?;
        let scan = plain(ScanAt::Latest, 0, 0, 0, &ranges);
        let (batches, refused) = answer(&mut dial().await?, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.batches = batches;
        Ok::<_, TestError>(())
    })?;
    w.kernel.bytes = held.iter().map(|iri| term_key(TENANT, iri)).collect();
    Ok(())
}

#[then(
    "one Entries frame answers them, `range` 0, `through` 49, `end` 1, holding 47 entries in the ranges' order"
)]
fn run_answered(w: &mut Steps) -> TestResult {
    let [batch] = w.kernel.batches.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.batches)));
    };
    assert_eq!(batch.range, 0);
    assert_eq!(batch.through, LOOKUPS - 1);
    assert_eq!(batch.end, EntriesEnd::Ended.number());
    assert!(batch.last, "the Scan's last frame");
    let keys: Vec<&Vec<u8>> = batch.entries.iter().map(|(key, _)| key).collect();
    assert_eq!(keys, w.kernel.bytes.iter().collect::<Vec<_>>());
    w.roles = None;
    Ok(())
}

#[when(
    "a list holds `<a>`, `<m>` and `<z>`, and one range names `after` `<m>` and another `after` `<n>`, which the list does not hold"
)]
fn resumed_ranges(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        for object in ["http://ex.org/a", "http://ex.org/m", "http://ex.org/z"] {
            let write = edge_write(SUBJECT, object);
            let _version = commit_at(&address(), PARTITION, &write).await?;
        }
        let mut client = dial().await?;
        let key = forward_key(&mut client, SUBJECT).await?;
        let whole = plain(ScanAt::Latest, 0, 0, 0, &[alone(key.clone())]);
        let (batches, _) = answer(&mut client, &whole, false).await?;
        let (_, value) = entry_of(&batches)?;
        let records = ObjectRecords::read(&value)?;
        let held: Vec<&[u8]> = records.iter().map(|record| record.encoding()).collect();
        // The list's own order names which is `<a>`, `<m>` and `<z>`.
        let [a, m, z] = held.as_slice() else {
            return Err(TestError::new(format!("{held:?}")));
        };
        let middle = EncodedTerm::from_bytes(m)?
            .as_id()
            .context("an object's ID")?;
        let n = EncodedTerm::id(SystemId::new(
            middle.type_prefix(),
            Bucket::new(middle.bucket().get() + 1)?,
            middle.sequence(),
        ));
        assert!(n.as_bytes() < *z, "<n> lies between <m> and <z>");
        let (first, past) = alone(key);
        let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, 0, 0);
        scan.push_range(ScanRange::new(PARTITION, &first, &past).with_after(m));
        scan.push_range(ScanRange::new(PARTITION, &first, &past).with_after(n.as_bytes()));
        let (answered, refused) = answer(&mut client, &scan, true).await?;
        assert!(refused.is_none(), "{refused:?}");
        w.kernel.bytes = vec![a.to_vec(), m.to_vec(), z.to_vec()];
        w.kernel.batches = answered;
        Ok(())
    })
}

#[then(
    "the first answers `<m>` and `<z>`, the second `<z>`, and neither reads the record of `<a>`"
)]
fn resumed_read(w: &mut Steps) -> TestResult {
    let [_, m, z] = w.kernel.bytes.as_slice() else {
        return Err(TestError::new("the objects"));
    };
    let of = |range: u32| -> TestResult<Vec<Vec<u8>>> {
        let batch = w
            .kernel
            .batches
            .iter()
            .find(|batch| batch.range == range)
            .context("the range's frame")?;
        crate::read_kernel_steps::objects_of(&batch.entries.first().context("its list")?.1)
    };
    assert_eq!(of(0)?, [m.clone(), z.clone()]);
    assert_eq!(of(1)?, std::slice::from_ref(z));
    w.roles = None;
    Ok(())
}

#[when(
    "a range's `after` is not one whole object encoding, and another range of the dictionary holds an `after`"
)]
fn malformed_after(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let list = key_prefix(Keyspace::Forward, false, TENANT, None, None);
    let dictionary = alone(term_key(TENANT, "http://ex.org/x"));
    let whole = EncodedTerm::id(played_id("http://ex.org/x")?);
    let cut = whole.as_bytes().split_last().context("an encoding")?.1;
    let mut cut_scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, 0, 0);
    cut_scan.push_range(ScanRange::new(PARTITION, &list, b"").with_after(cut));
    let mut dictionary_scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, NO_TRANSACTION, 0, 0);
    dictionary_scan.push_range(ScanRange::new(PARTITION, &list, b""));
    dictionary_scan.push_range(
        ScanRange::new(PARTITION, &dictionary.0, &dictionary.1).with_after(whole.as_bytes()),
    );
    run(async {
        let mut client = dial().await?;
        for scan in [&cut_scan, &dictionary_scan] {
            let (batches, refused) = answer(&mut client, scan, false).await?;
            assert!(batches.is_empty(), "before any frame: {batches:?}");
            w.kernel.refused.extend(refused);
        }
        Ok(())
    })
}

#[then(
    "each is refused with Error 1, the message naming the range, and no frame precedes the refusal"
)]
fn after_refused(w: &mut Steps) -> TestResult {
    let [cut, dictionary] = w.kernel.refused.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    protocol_naming(cut, "range 0");
    protocol_naming(dictionary, "range 1");
    w.roles = None;
    Ok(())
}

/// The skolem IRI of the bucket of an IRI partition 3 of 4 owns, which no dictionary holds.
fn foreign_skolem() -> TestResult<String> {
    let homed = subject_of(3, 4, TENANT)?;
    let bucket = iri_bucket(TENANT, &homed).context("a bucket")?;
    Ok(Names::new(tenant_base(TENANT))?
        .skolem_iri(bucket, 1)
        .as_str()
        .to_string())
}

#[when(
    "a transaction names in a Scan a skolem IRI whose home's store fails under the registration, which answers outcome 17"
)]
fn home_store_failed(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let skolem = foreign_skolem()?;
    let script = HomeScript::refusing(ErrorCode::Unavailable.number());
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (bind_stream, read) = bind_first(&mut client, 41).await?;
        let scan = term_scan(41, read, &skolem);
        client.send_only(&scan).await?;
        let scan_stream = client.last_stream();
        let mut sequence = EntriesSequence::new(scan.ranges());
        for _ in 0..2 {
            match next_frame(&mut client, &mut sequence).await? {
                Frame::Refused(refused) => w.kernel.refused.push(refused),
                Frame::Entries(batch) => w.kernel.batches.push(batch),
                Frame::Other(command) => {
                    return Err(TestError::new(format!("command {}", command.number())));
                }
            }
        }
        w.kernel.numbers = vec![bind_stream, scan_stream];
        Ok(())
    })
}

#[then("the Scan's stream and the Bind stream each carry code 17, and the range answers no entry")]
fn seventeen_on_both(w: &mut Steps) -> TestResult {
    assert!(w.kernel.batches.is_empty(), "{:?}", w.kernel.batches);
    let mut streams: Vec<u64> = w.kernel.refused.iter().map(|r| r.stream).collect();
    streams.sort_unstable();
    assert_eq!(streams, w.kernel.numbers, "{:?}", w.kernel.refused);
    for refused in &w.kernel.refused {
        assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    }
    w.roles = None;
    Ok(())
}

/// A transaction that only read: partition 1 holds no catalog of the tenant and `rdf:type` is homed elsewhere, so a Commit naming it
/// would be refused.
#[when(
    "a Scan of a transaction waits on a home's answer, the frontend cancels the read and sends the Commit"
)]
fn commit_behind_read(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let map = PlacementMap::even(4, NodeId::new(1))?;
    let rdf_type = iri_bucket(TENANT, RDF_TYPE).context("a bucket")?;
    assert_ne!(map.partition_of(rdf_type, Epoch::ZERO).get(), PARTITION);
    let iri = homed_at_three(5)?;
    let script = HomeScript::default();
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (bind_stream, read) = bind_first(&mut client, 42).await?;
        script.hold();
        client.send_only(&term_scan(42, read, &iri)).await?;
        let scan_stream = client.last_stream();
        script.asked().await?;
        client.send_on(scan_stream, &Cancel).await?;
        client
            .send_on(bind_stream, &Commit::new([0; 16], 0, 0))
            .await?;
        let early = compio::time::timeout(SILENCE, client.next_frame()).await;
        assert!(early.is_err(), "no answer before the home's reply");
        drop(early);
        script.release();
        let mut sequence = EntriesSequence::new([]);
        match next_frame(&mut client, &mut sequence).await? {
            Frame::Other(command) => {
                w.kernel.numbers = vec![u64::from(command.number())];
            }
            Frame::Refused(refused) => w.kernel.refused.push(refused),
            Frame::Entries(batch) => return Err(TestError::new(format!("{batch:?}"))),
        }
        Ok(())
    })
}

/// The bytes of a stored version's flags no version holds: reading them is a store's failure.
const UNREADABLE: [u8; 2] = [0, 0xff];

/// A partition holding healthy lists, stopped, its store given a list past every predicate's whose stored bytes cannot be read, and
/// started again: a Scan of them all reads on past the healthy ones and fails.
#[when("the partition's store fails reading a key of a Scan's range")]
fn scan_store_fails(w: &mut Steps) -> TestResult {
    let (util, settings, pid) = crate::committed_storage(w)?;
    let _stopped = util.stop_service(pid)?;
    let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
    let past = key_prefix(Keyspace::Forward, false, TENANT + 1, None, None);
    let failing = [first.as_slice(), &[0xff; 48]].concat();
    let data = settings
        .iter()
        .find(|(setting, _)| setting == "storage.data_dir")
        .map(|(_, directory)| directory.as_str())
        .context("the data directory")?;
    let store = Path::new(data)
        .join(format!("p{PARTITION}"))
        .join("r1")
        .join("store");
    write_raw_at(&store, DATA_KEYSPACE, &stored(&failing, 0), &UNREADABLE)?;
    let log = ultragraphdb_tests::scratch("storage-failing");
    let _restarted: u32 = crate::start_storage(&util, &settings, &log)?;
    w.roles = Some(util);
    let scan = plain(ScanAt::Latest, 0, 0, 0, &[(first, past)]);
    let (batches, refused) = run(async { answer(&mut dial().await?, &scan, false).await })?;
    w.kernel.batches = batches;
    w.kernel.refused.extend(refused);
    w.kernel.bytes = vec![failing];
    Ok(())
}

#[then(
    "the Scan is refused with code 17, the message naming the key, and no frame says the range ended"
)]
fn scan_refused(w: &mut Steps) -> TestResult {
    assert!(w.kernel.batches.is_empty(), "{:?}", w.kernel.batches);
    let [refused] = w.kernel.refused.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    let failing = w.kernel.bytes.first().context("the key that failed")?;
    let hex: String = failing.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(
        refused.message.contains(&format!("at the key {hex}:")),
        "{refused:?}"
    );
    w.roles = None;
    Ok(())
}

#[then(
    "the partition answers the Commit once the home's reply is in, and the Commit is not refused"
)]
fn commit_took_its_turn(w: &mut Steps) -> TestResult {
    assert!(w.kernel.refused.is_empty(), "{:?}", w.kernel.refused);
    assert_eq!(w.kernel.numbers, [u64::from(Command::Commit.number())]);
    w.roles = None;
    Ok(())
}

#[when(
    "the frontend sends a Write on a Bind stream, its references registered and its changes held by the partition"
)]
fn write_on_bind(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let (subject, object) = (subject_of(1, 4, TENANT)?, homed_at_three(6)?);
    let script = HomeScript::holding();
    run(async {
        play_home(&script).await?;
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (stream, _) = bind_first(&mut client, 43).await?;
        client
            .send_on(stream, &edge_write(&subject, &object))
            .await?;
        script.asked().await?;
        let early = compio::time::timeout(SILENCE, client.next_frame()).await;
        assert!(early.is_err(), "no Held before its references are answered");
        drop(early);
        script.release();
        let held = client.next_frame().await?;
        let view = held.view()?;
        if view.command() == Command::Error {
            let refusal = ErrorReplyView::from_frame(&view)?;
            return Err(TestError::new(refusal.message().to_string()));
        }
        assert_eq!(view.command().answering(Command::Write)?, Command::Held);
        assert_eq!(view.header().stream(), stream);
        assert_eq!(
            view.header().flags() & FLAG_LAST,
            0,
            "the stream stays open"
        );
        drop(held);
        let mut table = TermTable::new();
        let counted = table.iri(&subject);
        let predicate = table.iri("http://ex.org/count");
        let adding = Operation::new(
            table,
            CommitOperation::CounterAdd {
                graph: None,
                subject: counted,
                predicate,
                delta: 1,
            },
        );
        client.send_on(stream, &adding).await?;
        let operated = client.next_frame().await?;
        let answer_view = operated.view()?;
        if answer_view.command() == Command::Error {
            let refusal = ErrorReplyView::from_frame(&answer_view)?;
            return Err(TestError::new(refusal.message().to_string()));
        }
        assert_eq!(answer_view.command(), Command::Held, "an Operation too");
        drop(operated);
        // The stream stayed open: the Commit it takes commits what it holds.
        w.kernel
            .refused
            .extend(commit_answer(&mut client, stream).await?);
        Ok(())
    })
}

#[then(
    "the partition answers Held on the stream, which stays open; the dialling side's check accepts it, and the frontend relays nothing to the client"
)]
fn held_answered(w: &mut Steps) -> TestResult {
    assert!(w.kernel.refused.is_empty(), "{:?}", w.kernel.refused);
    w.roles = None;
    Ok(())
}

/// The refusal `client` reads next.
async fn refusal_next(client: &mut RoleClient) -> TestResult<Refused> {
    let mut sequence = EntriesSequence::new([]);
    match next_frame(client, &mut sequence).await? {
        Frame::Refused(refused) => Ok(refused),
        Frame::Entries(batch) => Err(TestError::new(format!("a frame: {batch:?}"))),
        Frame::Other(command) => Err(TestError::new(format!("command {}", command.number()))),
    }
}

/// A Bind of `kind` at `client`: its stream.
async fn bound(client: &mut RoleClient, kind: BindKind, identity: u8) -> TestResult<u64> {
    let bind = Bind::new(PARTITION, kind, TENANT, 0, PRINCIPAL, [identity; 16]);
    drop(client.request(&bind).await?);
    Ok(client.last_stream())
}

#[when(
    "a storage process receives a Write on a stream no Bind opened, a Commit on one of kind 2, and a Drop on one of kind 1"
)]
fn unbound_requests(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let write = edge_write(SUBJECT, OBJECT);
    let dropped = DropCommand::tenant_data(TENANT);
    let commit = Commit::new([0; 16], 0, 0);
    run(async {
        let mut client = dial().await?;
        client.send_only(&write).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        client.send_only(&dropped).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        let committing = bound(&mut client, BindKind::Drop, 44).await?;
        client.send_on(committing, &commit).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        let writing = bound(&mut client, BindKind::Drop, 45).await?;
        client.send_on(writing, &write).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        let aborting = bound(&mut client, BindKind::Drop, 46).await?;
        client.send_on(aborting, &Abort).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        let first_kind = bound(&mut client, BindKind::Transaction, 47).await?;
        client.send_on(first_kind, &dropped).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        // An Error ends the stream, and the transaction with it.
        let mut scan = of_transaction(47, 0, PRINCIPAL);
        let (first, past) = alone(term_key(TENANT, "http://ex.org/x"));
        scan.push_range(ScanRange::new(PARTITION, &first, &past));
        client.send_only(&scan).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        Ok(())
    })
}

#[then("each is refused with Error 1, the message naming the command")]
fn commands_named(w: &mut Steps) -> TestResult {
    let names = ["Write", "Drop", "Commit", "Write", "Abort", "Drop"];
    let (commands, ended) = w.kernel.refused.split_at(names.len());
    for (refused, name) in commands.iter().zip(names) {
        protocol_naming(refused, name);
    }
    let [ended] = ended else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    assert_eq!(ended.detail, 1, "the Drop ended its transaction: {ended:?}");
    w.roles = None;
    Ok(())
}

/// The values of a property [`too_large_record`]'s statement holds, each a property's 16-byte ID and its encoding, of at most this
/// many bytes in all, so together they pass a floor frame.
fn many_values() -> Vec<String> {
    const PROPERTY_MAX: usize = 26;
    (0..=MAX_FRAME_FLOOR / PROPERTY_MAX)
        .map(|at| format!("v{at:07}"))
        .collect()
}

/// A statement whose record a frame at the floor of `max_frame` cannot hold: how a Scan of its record is refused.
pub(crate) async fn too_large_record() -> TestResult<Refused> {
    let values = many_values();
    let properties: Vec<(&str, &str)> = values.iter().map(|at| (SINCE, at.as_str())).collect();
    let write = statement_write(&properties)?;
    let _version = commit_at(&address(), PARTITION, &write).await?;
    let target = Address::parse(&address())?;
    let taking = MAX_FRAME_FLOOR as u32;
    let mut small = RoleClient::dial_taking(&target, CLUSTER, "compute", SECRET, taking).await?;
    let statement = statement_of(&mut small).await?;
    let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(record_key(statement))]);
    small.send_only(&scan).await?;
    refusal_next(&mut small).await
}

#[when(
    "a transaction in partition 1 writes an edge to a skolem IRI whose bucket partition 2 owns, and partition 2's dictionary does not hold it"
)]
fn forged_edge(w: &mut Steps) -> TestResult {
    two_processes(w)?;
    let skolem = foreign_skolem()?;
    let subject = subject_of(1, 4, TENANT)?;
    run(async {
        let mut home = dial_at("tcp://127.0.0.1:17300").await?;
        let mut asked = Reference::request().in_tenant(TENANT);
        let _at = asked.register_iri(3, None, &skolem, 0);
        let reply = home.request(&asked).await?;
        let view = reply.view()?;
        let registered = ReferenceView::from_frame(&view)?;
        let outcome = registered.registration(0).context("the reply")?.outcome();
        w.kernel.numbers = vec![u64::from(outcome)];
        drop(reply);
        let mut client = dial_at("tcp://127.0.0.1:17200").await?;
        let (stream, read) = bind_first(&mut client, 48).await?;
        client
            .send_on(stream, &edge_write(&subject, &skolem))
            .await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        // The transaction ended with its Write.
        client.send_only(&term_scan(48, read, &subject)).await?;
        w.kernel.refused.push(refusal_next(&mut client).await?);
        Ok(())
    })
}

#[then(
    "the registration's outcome is 7, and the Write is refused with code 7, ending the transaction"
)]
fn forged_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.kernel.numbers,
        [u64::from(ErrorCode::Refused.number())],
        "the home's outcome"
    );
    let [write, ended] = w.kernel.refused.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.kernel.refused)));
    };
    assert_eq!(write.code, ErrorCode::Refused, "{write:?}");
    assert_eq!(ended.code, ErrorCode::Protocol, "{ended:?}");
    assert_eq!(ended.detail, 1, "{ended:?}");
    w.roles = None;
    Ok(())
}

/// Commits `<a> <p> <b>`, and waits for its epoch's close.
pub(crate) async fn committed_and_closed(client: &mut RoleClient) -> TestResult<u64> {
    let version = commit_at(&address(), PARTITION, &edge_write(SUBJECT, OBJECT)).await?;
    let epoch = Version::from_u64(version).epoch().get();
    closed_by(client, epoch).await?;
    Ok(epoch)
}

/// A Scan at `version` of `epoch`, of a list nobody holds.
pub(crate) fn scan_at(version: u64, epoch: u64) -> Scan {
    let first = key_prefix(Keyspace::Forward, false, TENANT, None, None);
    let (first, past) = alone(first);
    plain(
        ScanAt::ReadVersion,
        version,
        epoch,
        30_000,
        &[(first, past)],
    )
}

/// The commit of a later write, so the visibility watermark passes what came before: its version.
pub(crate) async fn commit_later(tag: u64) -> TestResult<u64> {
    let object = format!("http://ex.org/later{tag}");
    commit_at(&address(), PARTITION, &edge_write(SUBJECT, &object)).await
}
