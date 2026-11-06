/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios that need an image, run in Docker by the `image_bdd` target: each image loaded from its Bazel load target for
//! the daemon's own platform, the development image on its default profile, and the three role images on the `dev-net`
//! profile in three containers of one network. Every Docker command is bounded and its failure names what it ran.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_client_compio::{Client, ClientConfig, ReadLevel, TcpConnector, Transaction};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_wire::{
    DirectWrite, PatternSlot, QuadPattern, Read, RequestTerm, ResultView, TermTable, WriteChange,
};
use ultragraphdb_tests::{
    ADMIN_KEY, COMPUTE_BINARY, CONFIG_BINARY, FRONTEND_BINARY, READ_KEY, SECRET, START_SECONDS,
    STORAGE_BINARY, http_status, scratch, until_answering,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Set by the `image_bdd` target, whose scenarios run in Docker.
const IN_DOCKER: &str = "ULTRAGRAPHDB_IMAGE_TESTS";

/// The longest a Docker command may take: a load of the development image is the slowest, a few seconds.
const DOCKER_SECONDS: u64 = 120;

/// The user every role image and the development image run as, the glibc base's `nonroot`.
const NONROOT: &str = "65532";

/// The supervisor's binary, which only the development image holds.
const SUPERVISOR_BINARY: &str = "ultragraphdb_supervisor";

/// What a shell or a multi-call binary standing in for one is named.
const SHELLS: [&str; 6] = ["sh", "bash", "ash", "dash", "zsh", "busybox"];

/// Where the sandbox holds the shipped profiles.
const PROFILES: &str = "ultragraphdb/ultragraphdb_config/profiles";

/// The shipped seccomp profile.
const SECCOMP: &str = "bazel/oci_images/seccomp/ultragraphdb.json";

/// The storage image's index, naming one manifest per platform.
const STORAGE_INDEX: &str =
    "ultragraphdb/ultragraphdb_storage/ultragraphdb_storage_multiarch_index.json";

/// What the supervisor logs once every role is ready.
const EVERY_ROLE_READY: &str = "every role is ready";

/// The tenant the frontend's first start creates.
const TENANT: u64 = 0;

/// The administrator the frontend's first start creates from its bootstrap secret.
const ADMIN: &str = "admin";

/// The client endpoint's port in every container, as both profiles declare it.
const ENDPOINT_PORT: &str = "7080";

/// The statement each run writes and reads back.
const SUBJECT: &str = "http://ex.org/a";
const KNOWS: &str = "http://ex.org/knows";
const OBJECT: &str = "http://ex.org/b";

/// How many read-only transactions a read of a fresh write takes at most, each at a newer snapshot, 20 ms apart.
const SNAPSHOTS: u32 = 250;

static RUNS: AtomicU32 = AtomicU32::new(0);

/// One image: the load target that puts it in the daemon, and the name it loads as.
#[derive(Debug, Clone, Copy)]
struct Image {
    loader: &'static str,
    name: &'static str,
}

const GLIBC_BASE: Image = Image {
    loader: "bazel/oci_images/glibc_base/test_load.exe",
    name: "hansenmarvin/ultragraphdb_glibc_base:image-test",
};
const CONFIG_IMAGE: Image = Image {
    loader: "ultragraphdb/ultragraphdb_config/test_load.exe",
    name: "hansenmarvin/ultragraphdb_config:image-test",
};
const FRONTEND_IMAGE: Image = Image {
    loader: "ultragraphdb/ultragraphdb_frontend/test_load.exe",
    name: "hansenmarvin/ultragraphdb_frontend:image-test",
};
const STORAGE_IMAGE: Image = Image {
    loader: "ultragraphdb/ultragraphdb_storage/test_load.exe",
    name: "hansenmarvin/ultragraphdb_storage:image-test",
};
const COMPUTE_IMAGE: Image = Image {
    loader: "ultragraphdb/ultragraphdb_compute/test_load.exe",
    name: "hansenmarvin/ultragraphdb_compute:image-test",
};
const DEVELOPMENT_IMAGE: Image = Image {
    loader: "ultragraphdb/ultragraphdb_supervisor/development_test_load.exe",
    name: "hansenmarvin/ultragraphdb_development:image-test",
};

/// The seccomp profile a container runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seccomp {
    /// The container runtime's own default.
    Default,
    /// The profile the images ship with, allowing io_uring.
    Shipped,
}

/// What a scenario started in Docker, removed when it ends, and what it saw.
#[derive(Debug, Default)]
pub(crate) struct Containers {
    started: Vec<String>,
    network: Option<String>,
    /// The development container's log, once it said every role is ready.
    pub(crate) log: String,
    /// Where the scenario's client endpoint is published on this host.
    endpoint: String,
    /// The bootstrap administrator's password of the scenario's cluster.
    password: String,
    /// The cluster the scenario's roles name.
    cluster: u64,
    /// What each container of the three-host run takes as its `/secrets`.
    secrets: PathBuf,
    /// What the scenario's storage image holds and declares.
    image: ImageFacts,
}

/// What a role image holds and declares, read from the daemon and from the image's index.
#[derive(Debug, Default)]
struct ImageFacts {
    platforms: BTreeSet<String>,
    entrypoint: String,
    user: String,
    layers: Vec<String>,
    base_layers: Vec<String>,
    added: BTreeSet<String>,
    files: BTreeSet<String>,
}

/// A scenario that failed midway still removes what it started, so the next starts clean.
impl Drop for Containers {
    fn drop(&mut self) {
        for container in self.started.drain(..) {
            // The removal's outcome was the failed scenario's to report.
            drop(docker(&["rm", "-f", &container]));
        }
        if let Some(network) = self.network.take() {
            drop(docker(&["network", "rm", &network]));
        }
    }
}

/// Whether this run's scenarios run in Docker.
pub(crate) fn in_docker() -> bool {
    std::env::var_os(IN_DOCKER).is_some()
}

/// Whether a Docker daemon answers, so the `image_bdd` target can say plainly that it skipped, as scrith's container tests do.
pub(crate) fn docker_answers() -> bool {
    docker(&["version"]).is_ok()
}

/// Waits for `child` for up to `seconds`, killing it past that.
fn bounded(mut child: std::process::Child, what: &str, seconds: u64) -> TestResult<ExitStatus> {
    for _ in 0..seconds * 20 {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(child.kill());
    drop(child.wait());
    Err(TestError::new(format!(
        "{what} did not finish within {seconds} s"
    )))
}

/// Runs `program` on `args` with its output and errors in one file, which it returns, bounded by [`DOCKER_SECONDS`].
fn run(program: &Path, args: &[&str]) -> TestResult<String> {
    let at = RUNS.fetch_add(1, Ordering::Relaxed);
    let output = scratch(&format!("docker-{at}.out"));
    let file = File::create(&output)?;
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .spawn()
        .map_err(|error| TestError::new(format!("{} cannot start: {error}", program.display())))?;
    let what = format!("{} {}", program.display(), args.join(" "));
    let status = bounded(child, &what, DOCKER_SECONDS)?;
    let text = std::fs::read_to_string(&output)?;
    if status.success() {
        Ok(text)
    } else {
        Err(TestError::new(format!("{what} exited {status}: {text}")))
    }
}

/// Runs `docker` on `args`.
fn docker(args: &[&str]) -> TestResult<String> {
    run(Path::new("docker"), args)
}

/// Runs `docker` on `args` for what it does, not what it says.
fn act(args: &[&str]) -> TestResult {
    docker(args).map(drop)
}

/// The daemon's own platform, `linux/arm64` on Apple silicon.
fn platform() -> TestResult<String> {
    Ok(
        docker(&["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"])?
            .trim()
            .to_string(),
    )
}

/// Loads `image` into the daemon for its platform, from the test's runfiles.
fn load(image: Image) -> TestResult {
    let loader = std::env::current_dir()?.join(image.loader);
    run(&loader, &["--platform", &platform()?]).map(drop)
}

/// A name of this run's own for `role`.
fn container_name(role: &str) -> String {
    let at = RUNS.fetch_add(1, Ordering::Relaxed);
    format!("ugimg-{}-{at}-{role}", std::process::id())
}

/// What `format` reads of `image`'s inspection.
fn inspected(image: &str, format: &str) -> TestResult<String> {
    Ok(docker(&["image", "inspect", "--format", format, image])?
        .trim()
        .to_string())
}

/// The paths of every file `image` holds, as a container of it would see them.
fn files_of(image: &str) -> TestResult<BTreeSet<String>> {
    // An image with no command of its own still makes a container, which is never started.
    let container = docker(&["create", image, "/none"])?.trim().to_string();
    let archive = scratch(&format!("{container}.tar"));
    let exported = docker(&[
        "export",
        "--output",
        &archive.display().to_string(),
        &container,
    ]);
    drop(docker(&["rm", "-f", &container]));
    exported.map(drop)?;
    let listed = run(Path::new("tar"), &["-tf", &archive.display().to_string()])?;
    drop(std::fs::remove_file(&archive));
    Ok(listed
        .lines()
        .map(|line| line.trim_end_matches('/').to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

/// The platforms the index at `path` names a manifest for, as `os/architecture`.
fn platforms_of(path: &str) -> TestResult<BTreeSet<String>> {
    let index: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    index["manifests"]
        .as_array()
        .context("the index's manifests")?
        .iter()
        .map(|manifest| {
            let platform = &manifest["platform"];
            let os = platform["os"].as_str().context("a manifest's os")?;
            let architecture = platform["architecture"]
                .as_str()
                .context("a manifest's architecture")?;
            Ok(format!("{os}/{architecture}"))
        })
        .collect()
}

/// A JSON list of strings as read by `docker inspect`.
fn string_list(json: &str) -> TestResult<Vec<String>> {
    let list: Vec<String> = serde_json::from_str(json)?;
    Ok(list)
}

/// Loads the storage image and the glibc base, and notes what the image holds and declares.
pub(crate) fn storage_image(c: &mut Containers) -> TestResult {
    load(GLIBC_BASE)?;
    load(STORAGE_IMAGE)?;
    let image = &mut c.image;
    image.platforms = platforms_of(STORAGE_INDEX)?;
    image.entrypoint = inspected(STORAGE_IMAGE.name, "{{json .Config.Entrypoint}}")?;
    image.user = inspected(STORAGE_IMAGE.name, "{{.Config.User}}")?;
    image.layers = string_list(&inspected(STORAGE_IMAGE.name, "{{json .RootFS.Layers}}")?)?;
    image.base_layers = string_list(&inspected(GLIBC_BASE.name, "{{json .RootFS.Layers}}")?)?;
    image.files = files_of(STORAGE_IMAGE.name)?;
    let base = files_of(GLIBC_BASE.name)?;
    image.added = image.files.difference(&base).cloned().collect();
    Ok(())
}

/// The storage image holds its binary alone, as its entrypoint, on the glibc base, for both platforms, as 65532, with no shell.
pub(crate) fn storage_image_holds_its_binary(c: &Containers) -> TestResult {
    let image = &c.image;
    let both: BTreeSet<String> = ["linux/amd64", "linux/arm64"].map(String::from).into();
    assert_eq!(image.platforms, both, "the index's platforms");
    assert_eq!(
        string_list(&image.entrypoint)?,
        vec![format!("/{STORAGE_BINARY}")],
        "the entrypoint"
    );
    assert_eq!(image.user, NONROOT, "the user");
    assert!(
        image.layers.len() == image.base_layers.len() + 1
            && image.layers.starts_with(&image.base_layers),
        "the base's layers and one more: {:?} over {:?}",
        image.layers,
        image.base_layers
    );
    let alone: BTreeSet<String> = [STORAGE_BINARY.to_string()].into();
    assert_eq!(image.added, alone, "what the image adds to its base");
    for file in &image.files {
        let name = file.rsplit('/').next().unwrap_or(file);
        assert!(!SHELLS.contains(&name), "a shell: {file}");
        for other in [
            FRONTEND_BINARY,
            COMPUTE_BINARY,
            CONFIG_BINARY,
            SUPERVISOR_BINARY,
        ] {
            assert!(!name.starts_with(other), "another role's binary: {file}");
        }
    }
    Ok(())
}

/// The log of `container` so far, its output and errors together.
fn logs(container: &str) -> TestResult<String> {
    docker(&["logs", container])
}

/// Where `container` publishes its port `port` on this host, `127.0.0.1:<port>`.
fn published(container: &str, port: &str) -> TestResult<String> {
    let mapped = docker(&["port", container, &format!("{port}/tcp")])?;
    let first = mapped.lines().next().context("a published port")?;
    Ok(first.trim().replace("0.0.0.0", "127.0.0.1"))
}

/// Waits for the development container `container` to say every role is ready.
fn until_every_role_ready(c: &mut Containers, container: &str) -> TestResult {
    for _ in 0..START_SECONDS * 4 {
        c.log = logs(container)?;
        if c.log.contains(EVERY_ROLE_READY) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(TestError::new(format!(
        "the development container never said every role is ready:\n{}",
        c.log
    )))
}

/// The value of `key` in the first line naming it in `text`, a TOML profile file.
fn setting<'a>(text: &'a str, key: &str) -> TestResult<&'a str> {
    text.lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"'))
        })
        .ok_or_else(|| TestError::new(format!("{key} in the profile")))
}

/// The profile file `file` as shipped.
fn profile(file: &str) -> TestResult<String> {
    Ok(std::fs::read_to_string(Path::new(PROFILES).join(file))?)
}

/// Starts the development image on its default profile under `seccomp`, publishing its client endpoint, and waits until every
/// role is ready.
pub(crate) fn start_development(c: &mut Containers, seccomp: Seccomp) -> TestResult {
    load(DEVELOPMENT_IMAGE)?;
    let container = container_name("development");
    let security = format!(
        "seccomp={}",
        std::env::current_dir()?.join(SECCOMP).display()
    );
    let mut args = vec![
        "run",
        "--detach",
        "--name",
        &container,
        "--publish",
        "127.0.0.1::7080",
    ];
    if seccomp == Seccomp::Shipped {
        args.extend(["--security-opt", &security]);
    }
    args.push(DEVELOPMENT_IMAGE.name);
    act(&args)?;
    c.started.push(container.clone());
    until_every_role_ready(c, &container)?;
    c.endpoint = published(&container, ENDPOINT_PORT)?;
    c.cluster = setting(&profile("dev/frontend.toml")?, "id")?.parse()?;
    let secret = setting(&profile("dev/frontend.toml")?, "bootstrap_secret_file")?.to_string();
    let copied = scratch(&format!("{container}-bootstrap.secret"));
    act(&[
        "cp",
        &format!("{container}:{secret}"),
        &copied.display().to_string(),
    ])?;
    c.password = std::fs::read_to_string(&copied)?;
    Ok(())
}

/// What `docker diff` says the development container added under `/run/ultragraph`.
fn sockets_added(container: &str) -> TestResult<BTreeSet<String>> {
    Ok(docker(&["diff", container])?
        .lines()
        .filter_map(|line| line.strip_prefix("A "))
        .filter(|path| path.ends_with(".sock"))
        .map(str::to_string)
        .collect())
}

/// The octal field of a tar header at `at`, `width` bytes, as its value.
fn octal(header: &[u8], at: usize, width: usize) -> TestResult<u32> {
    let field = header.get(at..at + width).context("a tar header")?;
    let text = std::str::from_utf8(field)?.trim_matches(|c: char| c == '\0' || c == ' ');
    Ok(u32::from_str_radix(text, 8)?)
}

/// The mode, user and group of the directory `path` in `container`, read from the first header of the archive `docker cp`
/// makes of it: the owner the container's processes created it under, which a socket in it is reached through.
fn directory_owner(container: &str, path: &str) -> TestResult<(u32, u32, u32)> {
    let archive = docker(&["cp", &format!("{container}:{path}"), "-"])?;
    let header = archive.as_bytes();
    Ok((
        octal(header, 100, 8)? & 0o777,
        octal(header, 108, 8)?,
        octal(header, 116, 8)?,
    ))
}

/// The roles of the development container met at the Unix sockets the default profile names, and a write and a read succeed.
pub(crate) fn development_over_unix(c: &Containers) -> TestResult {
    let container = c.started.last().context("the development container")?;
    assert_eq!(
        inspected(DEVELOPMENT_IMAGE.name, "{{.Config.User}}")?,
        NONROOT,
        "the development image's user"
    );
    let nonroot: u32 = NONROOT.parse()?;
    assert_eq!(
        directory_owner(container, "/run/ultragraph")?,
        (0o700, nonroot, nonroot),
        "the sockets' directory: its mode, user and group"
    );
    let frontend = setting(&profile("dev/frontend.toml")?, "role_address")?.to_string();
    let compute = setting(&profile("dev/compute.toml")?, "role_address")?.to_string();
    let storage = profile("dev/storage.toml")?;
    assert_eq!(frontend, "unix:///run/ultragraph/frontend.sock");
    assert_eq!(compute, "unix:///run/ultragraph/compute.sock");
    let first: u32 = setting(&storage, "partition_first")?.parse()?;
    let count: u32 = setting(&storage, "partition_count")?.parse()?;
    let mut expected: BTreeSet<String> = [&frontend, &compute]
        .iter()
        .filter_map(|address| address.strip_prefix("unix://"))
        .map(str::to_string)
        .collect();
    expected.extend((first..first + count).map(|p| format!("/run/ultragraph/storage-p{p}.sock")));
    assert_eq!(sockets_added(container)?, expected, "{}", c.log);
    assert_eq!(objects(&write_and_read(c)?), [OBJECT], "{}", c.log);
    Ok(())
}

/// Every runtime of every role started on the driver `driver` names, and none on the other.
pub(crate) fn on_driver(c: &Containers, driver: &str, other: &str) -> TestResult {
    let line = format!("a runtime on the {driver} driver");
    let roles = ["frontend", "storage", "compute"].len();
    assert!(
        c.log.matches(&line).count() >= roles,
        "{line} for each role: {}",
        c.log
    );
    let refused = format!("a runtime on the {other} driver");
    assert!(!c.log.contains(&refused), "{refused}: {}", c.log);
    Ok(())
}

/// A client of the scenario's cluster, logged in as the bootstrap administrator.
async fn client(
    c: &Containers,
) -> TestResult<Client<TcpConnector, ScramLogin<fn() -> Result<String, ScramError>>>> {
    let nonces: fn() -> Result<String, ScramError> = system_nonce;
    let login = ScramLogin::new(ADMIN, &c.password, nonces)?;
    let config = ClientConfig::new(c.cluster, "ultragraphdb-image-tests");
    let mut connected = Client::new(TcpConnector::new(c.endpoint.parse()?), config, login, 16);
    connected.connect().await?;
    Ok(connected)
}

/// The first column of `rows`, the object the read leaves unbound.
fn objects(rows: &[Vec<String>]) -> Vec<&str> {
    rows.iter()
        .map(|row| row.first().map_or("", String::as_str))
        .collect()
}

/// The cells of `batch` as their terms' text.
fn rows_of(batch: &ResultView<'_>) -> Vec<Vec<String>> {
    (0..batch.rows())
        .map(|row| {
            (0..batch.columns())
                .map(|column| {
                    batch
                        .cell(column, row)
                        .and_then(|cell| batch.term(cell))
                        .and_then(|term| term.text())
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        })
        .collect()
}

/// The read of `<a> <knows> ?o` in `transaction`.
fn read_of(transaction: &Transaction) -> Read {
    let mut terms = TermTable::new();
    let subject = PatternSlot::Bound(terms.iri(SUBJECT));
    let predicate = PatternSlot::Bound(terms.iri(KNOWS));
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        subject,
        predicate,
        PatternSlot::Unbound,
    );
    Read::new(transaction.stream(), 0, terms, pattern, Vec::new())
}

/// Writes `<a> <knows> <b>` in one frame through the scenario's endpoint, then reads `<a> <knows> ?o` in read-only
/// transactions at the default level until a snapshot holds it: the rows of that read.
pub(crate) fn write_and_read(c: &Containers) -> TestResult<Vec<Vec<String>>> {
    Runtime::new()?.block_on(async {
        let mut connected = client(c).await?;
        let mut table = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: table.iri(SUBJECT),
            predicate: table.iri(KNOWS),
            object: table.intern(RequestTerm::Iri(OBJECT.to_string()))?,
        };
        let _written = connected
            .direct_write(DirectWrite::new(TENANT, table, vec![change]), None)
            .await?;
        for _ in 0..SNAPSHOTS {
            let reader = connected.begin(TENANT, true).await?;
            if reader.level() != ReadLevel::Snapshot {
                return Err(TestError::new(format!(
                    "the default level: {:?}",
                    reader.level()
                )));
            }
            let stream = connected.read(&reader, &read_of(&reader)).await?;
            let mut rows = Vec::new();
            while let Some(batch) = connected.next_batch(&stream, rows_of).await? {
                rows.extend(batch);
            }
            connected.abort(reader).await?;
            if !rows.is_empty() {
                return Ok(rows);
            }
            compio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(TestError::new(format!(
            "no snapshot of {SNAPSHOTS} read the write"
        )))
    })
}

/// The `dev-net` profile with every role at its own container's name in place of the loopback, every listener this host
/// reaches open on the container's interfaces, and its secrets and central file under `/secrets`.
fn over_three_hosts(text: &str) -> String {
    text.replace("tcp://127.0.0.1:7081", "tcp://frontend:7081")
        .replace("tcp://127.0.0.1:7082", "tcp://compute:7082")
        .replace("tcp://127.0.0.1:7100", "tcp://storage:7100")
        .replace("tcp://127.0.0.1:", "tcp://0.0.0.0:")
        .replace("/data/config/read.key", "/secrets/read.key")
        .replace("/data/config/admin.key", "/secrets/admin.key")
        .replace("/data/config/role.secret", "/secrets/role.secret")
        .replace("/data/config/bootstrap.secret", "/secrets/bootstrap.secret")
        .replace("/etc/ultragraphdb/profiles/dev-net", "/secrets/profile")
}

/// The scenario's `/secrets`: the moved `dev-net` profile, the config service's keys, the role secret and the bootstrap
/// administrator's password, readable by the user each image runs as.
fn write_secrets(c: &mut Containers) -> TestResult<PathBuf> {
    let at = RUNS.fetch_add(1, Ordering::Relaxed);
    let secrets = scratch(&format!("secrets-{at}"));
    std::fs::create_dir_all(secrets.join("profile"))?;
    for file in [
        "targets.toml",
        "frontend.toml",
        "storage.toml",
        "compute.toml",
    ] {
        let text = profile(&format!("dev-net/{file}"))?;
        std::fs::write(secrets.join("profile").join(file), over_three_hosts(&text))?;
    }
    std::fs::write(
        secrets.join("dev-net.toml"),
        over_three_hosts(&profile("dev-net.toml")?),
    )?;
    c.password = "the image tests' administrator password".to_string();
    for (file, text) in [
        ("read.key", READ_KEY),
        ("admin.key", ADMIN_KEY),
        ("role.secret", SECRET),
        ("bootstrap.secret", c.password.as_str()),
    ] {
        std::fs::write(secrets.join(file), text)?;
    }
    c.cluster = setting(&profile("dev-net/frontend.toml")?, "id")?.parse()?;
    Ok(secrets)
}

/// Creates a container called `alias` on the scenario's network, its `/data` a fresh directory of `owner`, `/secrets` copied
/// in, publishing `ports`, of the image and arguments `tail` names, then starts it.
fn start_on_network(
    c: &mut Containers,
    (alias, owner): (&str, &str),
    ports: &[&str],
    tail: &[String],
) -> TestResult<String> {
    let network = c.network.clone().context("the scenario's network")?;
    let container = container_name(alias);
    let data = format!("/data:uid={owner},gid={owner},mode=0700");
    let mut args = vec![
        "create".to_string(),
        "--name".to_string(),
        container.clone(),
        "--network".to_string(),
        network,
        "--network-alias".to_string(),
        alias.to_string(),
        "--tmpfs".to_string(),
        data,
    ];
    for port in ports {
        args.extend(["--publish".to_string(), format!("127.0.0.1::{port}")]);
    }
    args.extend(tail.iter().cloned());
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    act(&borrowed)?;
    c.started.push(container.clone());
    let secrets = c.secrets.display().to_string();
    act(&["cp", &secrets, &format!("{container}:/secrets")])?;
    act(&["start", &container])?;
    Ok(container)
}

/// The bootstrap of the role `role` of the `dev-net` target from the config service, then the role's image.
fn role_tail(role: &str, image: Image) -> Vec<String> {
    [
        ("ULTRAGRAPHDB_CONFIG_URL", "http://config:8090"),
        ("ULTRAGRAPHDB_CONFIG_TARGET", "dev-net"),
        ("ULTRAGRAPHDB_CONFIG_INSTANCE", role),
        ("ULTRAGRAPHDB_CONFIG_KEY_FILE", "/secrets/read.key"),
    ]
    .iter()
    .flat_map(|(name, value)| ["--env".to_string(), format!("{name}={value}")])
    .chain([image.name.to_string()])
    .collect()
}

/// The config service and the three role images in four containers of a network of their own, over `tcp://` alone, each
/// role ready, the frontend last.
pub(crate) fn start_three_hosts(c: &mut Containers) -> TestResult {
    for image in [CONFIG_IMAGE, FRONTEND_IMAGE, STORAGE_IMAGE, COMPUTE_IMAGE] {
        load(image)?;
    }
    let network = container_name("network");
    act(&["network", "create", &network])?;
    c.network = Some(network);
    c.secrets = write_secrets(c)?;
    let config_tail = [
        CONFIG_IMAGE.name.to_string(),
        "--config".to_string(),
        "/secrets/dev-net.toml".to_string(),
    ];
    let config = start_on_network(c, ("config", "65534"), &["8090"], &config_tail)?;
    until_answering(&published(&config, "8090")?, "/ready")?;
    let frontend = start_on_network(
        c,
        ("frontend", NONROOT),
        &[ENDPOINT_PORT, "8081"],
        &role_tail("frontend", FRONTEND_IMAGE),
    )?;
    let storage = start_on_network(
        c,
        ("storage", NONROOT),
        &["8083"],
        &role_tail("storage", STORAGE_IMAGE),
    )?;
    let compute = start_on_network(
        c,
        ("compute", NONROOT),
        &["8082"],
        &role_tail("compute", COMPUTE_IMAGE),
    )?;
    for (container, port) in [(&storage, "8083"), (&compute, "8082"), (&frontend, "8081")] {
        let at = published(container, port)?;
        until_answering(&at, "/ready").map_err(|error| {
            TestError::new(format!("{error}: {}", logs(container).unwrap_or_default()))
        })?;
    }
    assert_eq!(http_status(&published(&frontend, "8081")?, "/ready")?, 200);
    c.endpoint = published(&frontend, ENDPOINT_PORT)?;
    Ok(())
}

/// The write and read over the three hosts succeed, and their rows are those of the same write and read through the
/// development container's Unix sockets.
pub(crate) fn same_rows_over_both(c: &mut Containers) -> TestResult {
    let over_tcp = write_and_read(c)?;
    start_development(c, Seccomp::Default)?;
    let over_unix = write_and_read(c)?;
    assert_eq!(objects(&over_tcp), [OBJECT], "over tcp://");
    assert_eq!(over_tcp, over_unix, "over tcp:// and over unix://");
    Ok(())
}
