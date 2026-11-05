/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A home played by the test: a real storage process asks it as it would ask another, and it holds its answers or refuses them on
//! demand, which no real home can be made to do.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleCredentials, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_core_ids::{Sequence, SystemId, TypePrefix};
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, Join,
    ReferenceView,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{CLUSTER, FRONTEND_ROLE_ADDRESS, SECRET};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::TENANT;

/// The ports of partitions 3 and 4, which the home claims.
const PORTS: [(u32, u16); 2] = [(3, 17_300), (4, 17_301)];

/// How often a held answer looks whether it may go.
const POLL: Duration = Duration::from_millis(2);

/// What the test tells the home, and what it learned.
#[derive(Debug, Clone, Default)]
pub struct HomeScript {
    holding: Rc<Cell<bool>>,
    refusing: Rc<Cell<Option<u16>>>,
    asked: Rc<Cell<u32>>,
}

impl HomeScript {
    /// A home answering nothing until [`HomeScript::release`].
    pub fn holding() -> Self {
        let script = Self::default();
        script.holding.set(true);
        script
    }

    /// A home refusing every registration with `outcome`.
    pub fn refusing(outcome: u16) -> Self {
        let script = Self::default();
        script.refusing.set(Some(outcome));
        script
    }

    /// Holds every answer from now on, counting what reaches the home from none.
    pub fn hold(&self) {
        self.asked.set(0);
        self.holding.set(true);
    }

    /// Lets every held answer go.
    pub fn release(&self) {
        self.holding.set(false);
    }

    /// Waits until a Reference reached the home.
    pub async fn asked(&self) -> TestResult {
        for _ in 0..5_000 {
            if self.asked.get() > 0 {
                return Ok(());
            }
            compio::time::sleep(POLL).await;
        }
        Err(TestError::new("no Reference reached the home"))
    }
}

/// The ID the home gives `iri`.
pub fn played_id(iri: &str) -> TestResult<SystemId> {
    let bucket = iri_bucket(TENANT, iri).context("the IRI's bucket")?;
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        bucket,
        Sequence::new(1)?,
    ))
}

/// One connection at the home's address.
#[derive(Debug)]
struct PlayedHome {
    script: HomeScript,
}

impl PlayedHome {
    /// The reply to the Reference `view` on `stream`.
    fn reply(&self, stream: u64, view: &FrameView<'_>) -> TestResult<Vec<u8>> {
        let asked = ReferenceView::from_frame(view)?;
        let mut reply = asked.to_body()?.into_reply();
        for index in 0..asked.registrations().count() {
            let registration = asked.registration(index).context("a registration")?;
            let generation = u64::from(registration.group().is_some());
            let filled = match self.script.refusing.get() {
                Some(outcome) => reply.refuse(index, outcome),
                None => reply.accept(
                    index,
                    played_id(asked.iri(index).context("a registration's IRI")?)?,
                    generation,
                    0,
                ),
            };
            assert!(filled, "registration {index}");
        }
        let header = Header::new(CLUSTER, Command::Reference)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        Ok(reply.frame(header)?)
    }
}

impl Session for PlayedHome {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let refused =
            |message: String| Answer::Refuse(ErrorReply::new(ErrorCode::Refused, message));
        let view = match frame.view() {
            Ok(view) if view.command() == Command::Reference => view,
            Ok(view) => {
                return Answer::Refuse(ErrorReply::new(
                    ErrorCode::Unavailable,
                    format!("command {} is no home's", view.command().number()),
                ));
            }
            Err(error) => return refused(error.to_string()),
        };
        self.script.asked.set(self.script.asked.get() + 1);
        while self.script.holding.get() {
            compio::time::sleep(POLL).await;
        }
        match self.reply(stream, &view) {
            Ok(reply) => Answer::Reply(reply),
            Err(error) => refused(error.to_string()),
        }
    }
}

/// Credentials of the scenarios' role secret.
fn credentials() -> TestResult<RoleCredentials> {
    let file = std::env::temp_dir().join(format!("played-home-{}", std::process::id()));
    std::fs::write(&file, SECRET)?;
    Ok(RoleSecret::read(&file.display().to_string())?.credentials()?)
}

/// Plays the home of partitions 3 and 4 on this runtime, as `script` says: listening at their addresses, and joined.
pub async fn play_home(script: &HomeScript) -> TestResult {
    let held = credentials()?;
    let mut frontend = RoleClient::dial(
        &Address::parse(&format!("tcp://{FRONTEND_ROLE_ADDRESS}"))?,
        CLUSTER,
        "storage",
        SECRET,
    )
    .await?;
    for (partition, port) in PORTS {
        let address = format!("tcp://127.0.0.1:{port}");
        let listener = listen(&address).await?;
        let server =
            Server::new(CLUSTER, "storage", Endpoint::Partition)?.with_iterations(ROLE_ITERATIONS);
        let (held, script) = (held.clone(), script.clone());
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, held) = (server.clone(), held.clone());
                let session = PlayedHome {
                    script: script.clone(),
                };
                spawn(async move { serve_connection(carrier, &server, held, session).await })
                    .detach();
            }
        })
        .detach();
        drop(
            frontend
                .request(&Join::storage(address, vec![partition]))
                .await?,
        );
    }
    Ok(())
}
