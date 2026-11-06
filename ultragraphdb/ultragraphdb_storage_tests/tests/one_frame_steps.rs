/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's hosting of DirectWrite, ConditionalWrite and a Drop, over a real storage process and a real frontend, the frontend's
//! side of each played by this test on the partition's role address.

use cucumber::{then, when};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Begin, Bind, BindKind, BodyView, Commit, CommitOperation, CommitView, ConditionalWrite,
    Declared, DeclaredPredicate, DirectWrite, DropCommand, EntriesSequence, ErrorCode, RequestTerm,
    ScanAt, TermTable, Write, WriteChange,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, answering, frontend_holding,
    role, scratch, storage_holding, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::read_kernel_steps::{
    Frame, PARTITION, PRINCIPAL, alone, answer, dial, forward_key, id_of, next_frame,
    one_partition, plain, run,
};
use crate::{PREDICATE, Steps, TENANT, commit_at, edge_write, subject_of};

const SESSION: [u8; 16] = [7; 16];
const UNIQUE: &str = "http://ex.org/id";
const NODE: &str = "http://ex.org/x";

/// A DirectWrite of `subject`'s edge to `object`.
fn direct(subject: &str, object: &str) -> DirectWrite {
    let mut terms = TermTable::new();
    let change = insert(&mut terms, subject, PREDICATE, object);
    DirectWrite::new(TENANT, terms, vec![change]).with_principal(PRINCIPAL)
}

fn insert(terms: &mut TermTable, subject: &str, predicate: &str, object: &str) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        object: terms.iri(object),
    }
}

/// What the next frame on `client` says: a Commit's version, its token and first node, or the refusal.
async fn committed(
    client: &mut RoleClient,
) -> TestResult<Result<(u64, Option<SessionToken>, Option<(String, bool)>), (ErrorCode, String)>> {
    let frame = client.next_frame().await?;
    let view = frame.view()?;
    if view.command() != ultragraphdb_protocol_wire::Command::Commit {
        let error = ultragraphdb_protocol_wire::ErrorReplyView::from_frame(&view)?;
        return Ok(Err((error.code(), error.message().to_string())));
    }
    let reply = CommitView::from_frame(&view)?;
    let node = reply
        .result(0)
        .and_then(|result| Some((reply.node(0)?.to_string(), result.created())));
    Ok(Ok((reply.commit_version(), reply.token().cloned(), node)))
}

/// A frontend holding the bootstrap administrator, and one partition.
fn with_an_administrator(w: &mut Steps) -> TestResult {
    let util = crate::services()?;
    let file = scratch("admin-secret");
    std::fs::write(&file, "pw")?;
    let settings = with(
        frontend_holding()?,
        "metadata.bootstrap_secret_file",
        &file.display().to_string(),
    );
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &settings,
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    let _storage = crate::start_storage(&util, &storage_holding()?, &scratch("storage-unique"))?;
    w.roles = Some(util);
    Ok(())
}

/// Declares `UNIQUE` unique through the client endpoint, and waits until its version reached the partition: a DirectWrite of it refused.
async fn declared_unique() -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{CLIENT_ADDRESS}"))?;
    let mut admin = RoleClient::dial(&target, CLUSTER, "admin", "pw").await?;
    drop(admin.request(&Begin::new(TENANT, 0, false)).await?);
    let stream = admin.last_stream();
    let declared = DeclaredPredicate::new(UNIQUE).with_unique(true);
    let write = Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(declared))],
    );
    admin.send_on(stream, &write).await?;
    admin.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
    let reply = admin.next_frame().await?;
    let view = reply.view()?;
    let _committed = CommitView::from_frame(&view)?;
    drop(reply);
    let subject = subject_of(1, 1, TENANT)?;
    let mut client = dial().await?;
    for _ in 0..500 {
        let mut terms = TermTable::new();
        let change = insert(&mut terms, &subject, UNIQUE, "http://ex.org/o");
        let probe = DirectWrite::new(TENANT, terms, vec![change]).with_principal(PRINCIPAL);
        client.send_only(&probe).await?;
        if committed(&mut client).await?.is_err() {
            return Ok(client);
        }
        compio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Err(TestError::new("the version never reached the partition"))
}

#[when("a client sends a DirectWrite inserting `<a> <knows> <b>` through its session")]
fn insert_in_one_request(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let mut client = dial().await?;
        let write = direct(&subject, "http://ex.org/b").with_session(SESSION, 1);
        client.send_only(&write).await?;
        let (version, token, _) = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        assert!(version > 0, "the commit version");
        assert!(token.is_some(), "the session token");
        let key = forward_key(&mut client, &subject).await?;
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]);
        let (batches, refused) = answer(&mut client, &scan, false).await?;
        assert!(refused.is_none(), "{refused:?}");
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.entries.len())
                .sum::<usize>(),
            1
        );
        Ok(())
    })
}

#[then(
    "the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply"
)]
fn committed_in_one_request(w: &mut Steps) {
    w.roles = None;
}

#[when(
    "a DirectWrite holds a valid insert and an insert of the predicate `id`, which the catalog declares unique"
)]
fn unique_in_a_direct_write(w: &mut Steps) -> TestResult {
    with_an_administrator(w)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let mut client = declared_unique().await?;
        let mut terms = TermTable::new();
        let changes = vec![
            insert(&mut terms, &subject, PREDICATE, "http://ex.org/b"),
            insert(&mut terms, &subject, UNIQUE, "http://ex.org/o"),
        ];
        let write = DirectWrite::new(TENANT, terms, changes).with_principal(PRINCIPAL);
        client.send_only(&write).await?;
        let refused = committed(&mut client).await?.err().context("a refusal")?;
        assert_eq!(refused.0, ErrorCode::Refused);
        assert!(
            refused.1.contains(UNIQUE) && refused.1.contains("ConditionalWrite"),
            "{}",
            refused.1
        );
        Ok(())
    })
}

#[then("it is refused with an error naming `id` and ConditionalWrite, and neither insert is held")]
fn unique_refused(w: &mut Steps) {
    w.roles = None;
}

#[when("two DirectWrites insert `<a> <knows> <b>` concurrently")]
fn identical_inserts(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let mut client = dial().await?;
        let write = direct(&subject, "http://ex.org/b");
        client.send_only(&write).await?;
        client.send_only(&write).await?;
        let first = committed(&mut client).await?;
        let second = committed(&mut client).await?;
        assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
        let key = forward_key(&mut client, &subject).await?;
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]);
        let (batches, _) = answer(&mut client, &scan, false).await?;
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.entries.len())
                .sum::<usize>(),
            1,
            "held once"
        );
        Ok(())
    })
}

#[then("both are answered with success, whichever executed first, and the statement is held once")]
fn both_succeed(w: &mut Steps) {
    w.roles = None;
}

/// A ConditionalWrite claiming `value` of `UNIQUE`.
fn claim(value: &str) -> TestResult<ConditionalWrite> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: terms.iri(UNIQUE),
        value: terms.intern(RequestTerm::Literal {
            lexical: value.to_string(),
            datatype: None,
        })?,
        properties: Vec::new(),
    };
    Ok(ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation]).with_principal(PRINCIPAL))
}

#[when("two ConditionalWrites each insert-if-absent a node with `id` `\"x\"` concurrently")]
fn two_claims(w: &mut Steps) -> TestResult {
    with_an_administrator(w)?;
    run(async {
        let mut client = declared_unique().await?;
        client.send_only(&claim("x")?).await?;
        client.send_only(&claim("x")?).await?;
        let first = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        let second = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        let (first, second) = (first.2.context("a node")?, second.2.context("a node")?);
        assert_eq!(first.0, second.0, "the same node");
        assert!(first.1 != second.1, "created once: {first:?} {second:?}");
        Ok(())
    })
}

#[then("both name the same node, created once")]
fn claimed_once(w: &mut Steps) {
    w.roles = None;
}

#[when(
    "a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction commits"
)]
fn node_found(w: &mut Steps) -> TestResult {
    with_an_administrator(w)?;
    run(async {
        let mut client = declared_unique().await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(NODE),
            predicate: terms.iri(UNIQUE),
            object: terms.intern(RequestTerm::Literal {
                lexical: "held".to_string(),
                datatype: None,
            })?,
        };
        let other = CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: terms.iri(UNIQUE),
            value: terms.intern(RequestTerm::Literal {
                lexical: "other".to_string(),
                datatype: None,
            })?,
            properties: Vec::new(),
        };
        let held = ConditionalWrite::new(TENANT, terms, vec![change], vec![other])
            .with_principal(PRINCIPAL);
        client.send_only(&held).await?;
        let _held = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        let id = id_of(&mut client, NODE).await?.to_be_bytes();
        client.send_only(&claim("held")?).await?;
        let frame = client.next_frame().await?;
        let view = frame.view()?;
        let reply = CommitView::from_frame(&view)?;
        assert_eq!(reply.node(0), Some(NODE));
        let holds = frame.bytes().windows(id.len()).any(|window| window == id);
        assert!(!holds, "a system ID in the reply");
        Ok(())
    })
}

#[then("the Commit reply names `http://ex.org/x` by its text, and holds no system ID")]
fn node_named(w: &mut Steps) {
    w.roles = None;
}

#[when("a ConditionalWrite holds a compare-and-set whose expected value does not hold")]
fn failed_condition(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let mut client = dial().await?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::CompareAndSet {
            graph: None,
            subject: terms.iri(&subject),
            predicate: terms.iri(PREDICATE),
            expected: terms.iri("http://ex.org/expected"),
            new: terms.iri("http://ex.org/new"),
        };
        let write = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL);
        client.send_only(&write).await?;
        let refused = committed(&mut client).await?.err().context("a refusal")?;
        assert_eq!(refused.0, ErrorCode::OperationFailed, "{}", refused.1);
        Ok(())
    })
}

#[then(
    "it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written"
)]
fn condition_answered(w: &mut Steps) {
    w.roles = None;
}

#[when("a client loses the reply to request 7 of its session, a DirectWrite, and sends it again")]
fn retried_write(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let mut client = dial().await?;
        let write = direct(&subject, "http://ex.org/b").with_session(SESSION, 7);
        client.send_only(&write).await?;
        let first = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        client.send_only(&write).await?;
        let second = committed(&mut client)
            .await?
            .map_err(|refusal| TestError::new(refusal.1))?;
        assert_eq!(first.0, second.0, "the same commit version");
        assert_eq!(first.1, second.1, "the same token");
        Ok(())
    })
}

#[then("it receives the same reply and commit version, and the writes are held once")]
fn retried_once(w: &mut Steps) {
    w.roles = None;
}

#[when("a Bind names tenant 9, for which the partition's placement map lists no run")]
fn unknown_tenant_bind(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    run(async {
        let mut client = dial().await?;
        let bind = Bind::new(PARTITION, BindKind::Transaction, 9, 0, PRINCIPAL, [9; 16]);
        client.send_only(&bind).await?;
        let mut sequence = EntriesSequence::new([]);
        let Frame::Refused(refused) = next_frame(&mut client, &mut sequence).await? else {
            return Err(TestError::new("a refusal"));
        };
        assert_eq!(refused.code, ErrorCode::Protocol);
        assert!(refused.message.contains("tenant 9"), "{}", refused.message);
        // The identity stays free, so it held no transaction.
        let held = Bind::new(
            PARTITION,
            BindKind::Transaction,
            TENANT,
            0,
            PRINCIPAL,
            [9; 16],
        );
        drop(client.request(&held).await?);
        Ok(())
    })
}

#[then(
    "the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction"
)]
fn unknown_tenant_refused(w: &mut Steps) {
    w.roles = None;
}

#[when("a tenant's data is dropped")]
fn tenant_data_dropped(w: &mut Steps) -> TestResult {
    one_partition(w, None)?;
    let subject = subject_of(1, 1, TENANT)?;
    run(async {
        let address = crate::read_kernel_steps::address();
        let _version = commit_at(
            &address,
            PARTITION,
            &edge_write(&subject, "http://ex.org/b"),
        )
        .await?;
        let mut client = dial().await?;
        let key = forward_key(&mut client, &subject).await?;
        let bind = Bind::new(PARTITION, BindKind::Drop, TENANT, 0, PRINCIPAL, [8; 16]);
        drop(client.request(&bind).await?);
        let stream = client.last_stream();
        client
            .send_on(stream, &DropCommand::tenant_data(TENANT))
            .await?;
        let reply = client.next_frame().await?;
        let view = reply.view()?;
        assert_eq!(view.command(), ultragraphdb_protocol_wire::Command::Drop);
        drop(reply);
        let scan = plain(ScanAt::Latest, 0, 0, 0, &[alone(key)]);
        let (batches, _) = answer(&mut client, &scan, false).await?;
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.entries.len())
                .sum::<usize>(),
            0
        );
        Ok(())
    })
}

#[then("its predicates hold no data and its catalog is unchanged")]
fn data_dropped(w: &mut Steps) {
    w.roles = None;
}
