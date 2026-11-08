/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a partition's core exchanges beyond its process (`PROTOCOL.md` §13.15): a Reference reaching it as a home, answered once
//! each registration is durable, and a write's registrations at a home in another process, whose address it asks of the frontend.
//! The frontend and the other home are played by the test.

use std::collections::BTreeMap;

use compio::runtime::spawn;
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, Server, Session, listen, serve_connection,
};
use ultragraphdb_core_ids::{Sequence, SystemId, TypePrefix};
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Commit, CommitView, ErrorCode, ErrorReply, FLAG_LAST,
    FrameView, Header, Members, Reference, ReferenceView,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::TestDir;

use crate::types::partition_core::core_harness::{
    CLUSTER, DUE, PARTITION, Setup, TENANT, ex, free_address, id_of, next_frame, on_core,
    refusal_of, role_cluster,
};
use crate::types::partition_core::partition_core_tests::parked_write;

/// What a home answers each registration with: 0 accepts it, any other number refuses it with that outcome.
const ACCEPTED: u16 = 0;
const REFUSED: u16 = 7;
/// What the core refuses a registration of another partition with (`PROTOCOL.md` §13.15).
const WRONG_PARTITION: u16 = 26;

/// The frontend's address, which a core of one partition never dials.
fn no_frontend() -> TestResult<Address> {
    Ok(Address::parse("tcp://127.0.0.1:1")?)
}

/// A Reference of [`TENANT`] registering each IRI of `registrations` at its partition, naming no group.
fn reference(registrations: &[(u32, &str)]) -> Reference {
    let mut reference = Reference::request().in_tenant(TENANT);
    for (partition, iri) in registrations {
        let _index = reference.register_iri(*partition, None, iri, 0);
    }
    reference
}

/// Each registration's outcome and entity in the Reference reply `frame`.
fn answered(frame: &ReceivedFrame) -> TestResult<Vec<(u16, Option<SystemId>)>> {
    let view = frame.view()?;
    let reply = ReferenceView::from_frame(&view)?;
    assert!(reply.is_reply());
    Ok(reply
        .registrations()
        .map(|registration| (registration.outcome(), registration.entity()))
        .collect())
}

/// A Reference reaching the partition as a home registers each entity it owns, answering each with the ID its dictionary holds,
/// once all are durable; one of another partition it refuses with outcome 26.
#[test]
fn test_a_reference_registers_each_entity_of_its_partition() -> TestResult {
    on_core(
        "remote-home",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let (a, b, c) = (ex("a"), ex("b"), ex("c"));
            let asked = reference(&[(PARTITION, &a), (PARTITION, &b), (PARTITION + 1, &c)]);
            let reply = compio::time::timeout(DUE, client.request(&asked))
                .await
                .context("the home's reply")??;
            let outcomes = answered(&reply)?;
            drop(reply);
            let expected = [
                (ACCEPTED, Some(id_of(&mut client, &a).await?)),
                (ACCEPTED, Some(id_of(&mut client, &b).await?)),
                (WRONG_PARTITION, None),
            ];
            assert_eq!(outcomes, expected);
            Ok(())
        },
    )
}

/// A Reference naming no entity of the partition is answered at once, each registration refused with outcome 26.
#[test]
fn test_a_reference_of_no_entity_here_is_answered_at_once() -> TestResult {
    on_core(
        "remote-none",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let asked = reference(&[(PARTITION + 1, &ex("c"))]);
            let reply = compio::time::timeout(DUE, client.request(&asked))
                .await
                .context("the home's reply")??;
            assert_eq!(answered(&reply)?, [(WRONG_PARTITION, None)]);
            Ok(())
        },
    )
}

/// References asked together are each answered on their own stream, with their own registrations.
#[test]
fn test_references_asked_together_are_each_answered() -> TestResult {
    on_core(
        "remote-together",
        Setup::default(),
        no_frontend()?,
        async |core| {
            let mut client = core.dial().await?;
            let names = ["a", "b", "c", "d"].map(ex);
            let mut streams = BTreeMap::new();
            for pair in names.chunks(2) {
                let registrations: Vec<(u32, &str)> =
                    pair.iter().map(|iri| (PARTITION, iri.as_str())).collect();
                client.send_only(&reference(&registrations)).await?;
                let _none = streams.insert(client.last_stream(), pair.to_vec());
            }
            let mut replies = BTreeMap::new();
            for _ in 0..streams.len() {
                let frame = next_frame(&mut client).await?;
                let stream = frame.view()?.header().stream();
                let _none = replies.insert(stream, answered(&frame)?);
            }
            for (stream, iris) in &streams {
                let mut expected = Vec::new();
                for iri in iris {
                    expected.push((ACCEPTED, Some(id_of(&mut client, iri).await?)));
                }
                assert_eq!(replies.get(stream), Some(&expected), "stream {stream}");
            }
            Ok(())
        },
    )
}

/// The ID a played home gives `iri`: the first of the IRI's bucket.
fn played_id(iri: &str) -> TestResult<SystemId> {
    let bucket = iri_bucket(TENANT, iri).context("the IRI's bucket")?;
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        bucket,
        Sequence::new(1)?,
    ))
}

/// A frontend and a home in another process, played: the frontend names the home's address for partition 2, a compute process's
/// beside it, and the home answers each registration with `outcome`, an accepted one with the IRI's played ID.
#[derive(Debug, Clone)]
struct PlayedPeer {
    home: String,
    outcome: u16,
}

impl PlayedPeer {
    fn reply(&self, stream: u64, view: &FrameView<'_>) -> TestResult<Vec<u8>> {
        let header = Header::new(CLUSTER, view.command())
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        if view.command() == Command::Members {
            let mut members = Members::new();
            members.push_compute("tcp://127.0.0.1:2");
            members.push_storage(PARTITION + 1, self.home.clone());
            return Ok(members.frame(header)?);
        }
        let asked = ReferenceView::from_frame(view)?;
        let mut reply = asked.to_body()?.into_reply();
        for index in 0..asked.len() {
            let filled = if self.outcome == ACCEPTED {
                let iri = asked.iri(index).context("a registration's IRI")?;
                let registration = asked.registration(index).context("a registration")?;
                // A registration for a group starts its member's generations at 1.
                let generation = u64::from(registration.group().is_some());
                reply.accept(index, played_id(iri)?, generation, 0)
            } else {
                reply.refuse(index, self.outcome)
            };
            assert!(filled, "registration {index}");
        }
        Ok(reply.frame(header)?)
    }
}

impl Session for PlayedPeer {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        match frame
            .view()
            .map_err(|error| TestError::new(error.to_string()))
            .and_then(|view| self.reply(stream, &view))
        {
            Ok(reply) => Answer::Reply(reply),
            Err(error) => Answer::Refuse(ErrorReply::new(ErrorCode::Refused, error.to_string())),
        }
    }
}

/// Plays `peer` at the frontend's address and the home's, with the credentials of `dir`'s role secret.
async fn play(peer: &PlayedPeer, frontend: &str, dir: &TestDir) -> TestResult {
    let credentials = role_cluster(dir.path())?.secret().credentials()?;
    for (address, endpoint) in [
        (frontend, Endpoint::Frontend),
        (peer.home.as_str(), Endpoint::Partition),
    ] {
        let listener = listen(address).await?;
        let server = Server::new(CLUSTER, "played", endpoint)?.with_iterations(ROLE_ITERATIONS);
        let (peer, credentials) = (peer.clone(), credentials.clone());
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials, peer) =
                    (server.clone(), credentials.clone(), peer.clone());
                spawn(async move { serve_connection(carrier, &server, credentials, peer).await })
                    .detach();
            }
        })
        .detach();
    }
    Ok(())
}

/// Runs `test` beside a core of partition 1 of 2, whose frontend and partition 2's home are played as `outcome` says.
fn with_played_home(
    name: &str,
    outcome: u16,
    test: impl AsyncFnOnce(RoleClient) -> TestResult,
) -> TestResult {
    let frontend = free_address()?;
    let peer = PlayedPeer {
        home: free_address()?,
        outcome,
    };
    let dir = TestDir::new(name);
    let setup = Setup {
        partitions: 2,
        ..Setup::default()
    };
    on_core(name, setup, Address::parse(&frontend)?, async |core| {
        play(&peer, &frontend, &dir).await?;
        test(core.dial().await?).await
    })
}

/// A Write naming an entity a home in another process owns is held once that home accepts its registration: the core asks the
/// frontend where the home listens, registers there, and the transaction commits.
#[test]
fn test_a_write_is_held_once_a_home_in_another_process_accepts() -> TestResult {
    with_played_home("remote-accepted", ACCEPTED, async |mut client| {
        let stream = parked_write(&mut client).await?;
        let held = next_frame(&mut client).await?;
        let view = held.view()?;
        assert_eq!(
            (view.header().stream(), view.command()),
            (stream, Command::Held),
            "{:?}",
            refusal_of(&held)?
        );
        drop(held);
        client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
        let committed = next_frame(&mut client).await?;
        assert!(
            refusal_of(&committed)?.is_none(),
            "{:?}",
            refusal_of(&committed)?
        );
        assert!(CommitView::from_frame(&committed.view()?)?.commit_version() > 0);
        Ok(())
    })
}

/// A home in another process refusing a registration ends the transaction, its Bind stream told so with code 7 (`PROTOCOL.md`
/// §13.15).
#[test]
fn test_a_homes_refusal_ends_the_transaction_on_its_stream() -> TestResult {
    with_played_home("remote-refused", REFUSED, async |mut client| {
        let stream = parked_write(&mut client).await?;
        let told = next_frame(&mut client).await?;
        let told = refusal_of(&told)?.context("an Error")?;
        assert_eq!((told.stream, told.code), (stream, ErrorCode::Refused));
        Ok(())
    })
}
