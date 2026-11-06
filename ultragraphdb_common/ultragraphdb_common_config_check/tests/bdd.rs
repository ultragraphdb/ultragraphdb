/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The validation's scenarios, bound to `features/`: settings written to a scratch folder and checked as an administrator
//! would.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use cucumber::{World, then, when};
use ultragraphdb_common_config_check::{
    CheckReport, Defect, TargetFiles, check_central, check_target, instances, read_central,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 2\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\n";
const COMPUTE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[compute]\nrole_address = \"tcp://10.0.0.3:7082\"\n";

#[derive(Debug, Default, World)]
struct Steps {
    folder: Option<PathBuf>,
    report: CheckReport,
    defects: Vec<Defect>,
    target: TargetFiles,
}

fn folder(w: &mut Steps) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = root.join(format!(
        "config-check-{}-{}",
        std::process::id(),
        FOLDERS.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path)?;
    w.folder = Some(path.clone());
    Ok(path)
}

/// A valid target: one frontend, two storage instances of one partition each, one compute.
fn valid() -> TargetFiles {
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "ci_frontend.toml", FRONTEND.as_bytes().to_vec());
    target.insert("storage", "ci_storage.toml", STORAGE.as_bytes().to_vec());
    target.insert(
        "storage_1",
        "ci_storage_1.toml",
        b"[storage]\npartition_first = 1\nrole_address = \"tcp://10.0.0.2:7000\"\n".to_vec(),
    );
    target.insert(
        "storage_2",
        "ci_storage_2.toml",
        b"[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.4:7000\"\n".to_vec(),
    );
    target.insert(
        "compute_1",
        "ci_compute_1.toml",
        COMPUTE.as_bytes().to_vec(),
    );
    target
}

/// `target` with `key`'s file holding `text` instead.
fn with(mut target: TargetFiles, key: &str, text: &str) -> TargetFiles {
    let label = target.label(key).unwrap_or(key).to_string();
    target.insert(key, label, text.as_bytes().to_vec());
    target
}

/// Writes `target` and a central file declaring it into a fresh folder: the central file's path.
fn write_out(w: &mut Steps, target: &TargetFiles) -> TestResult<PathBuf> {
    let path = folder(w)?;
    let mut central = format!("[{}]\n", target.name());
    for (key, label, bytes) in target.files() {
        central.push_str(&format!("{key} = \"{label}\"\n"));
        std::fs::write(path.join(label), bytes)?;
    }
    let file = path.join("targets.toml");
    std::fs::write(&file, central)?;
    Ok(file)
}

fn listing(defects: &[Defect]) -> String {
    defects
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn says(w: &Steps, words: &[&str]) {
    let text = listing(&w.defects);
    for word in words {
        assert!(text.contains(word), "{word} in: {text}");
    }
}

#[when(
    "target `ci` holds `storage` with `partition_count = 2` and `storage_2` with `partition_first = 3`"
)]
fn overrides(w: &mut Steps) {
    let shared = STORAGE.replace("partition_count = 1", "partition_count = 2");
    w.target = with(valid(), "storage", &shared);
    w.target.insert(
        "storage_2",
        "ci_storage_2.toml",
        b"[storage]\npartition_first = 3\nrole_address = \"tcp://10.0.0.4:7000\"\n".to_vec(),
    );
}

#[then(
    "`storage_2` receives both settings, and `storage_1`, whose file names `partition_first = 1`, receives its own"
)]
fn merged(w: &mut Steps) -> TestResult {
    let all = instances(&w.target);
    let of = |name: &str| {
        all.iter()
            .find(|instance| instance.name() == name)
            .context(name)
    };
    let second = of("storage_2")?.config().context("settings")?;
    assert_eq!(second.storage().partition_count(), Some(2));
    assert_eq!(second.storage().partition_first(), Some(3));
    let first = of("storage_1")?.config().context("settings")?;
    assert_eq!(first.storage().partition_first(), Some(1));
    assert!(
        all.iter().all(|instance| instance.name() != "storage"),
        "the role's file is shared"
    );
    assert_eq!(
        of("storage_2")?.rendered(),
        instances(&w.target)
            .iter()
            .find(|i| i.name() == "storage_2")
            .context("again")?
            .rendered()
    );
    Ok(())
}

#[when("a revision's `storage_1` lacks `data_dir` and whose `compute_1` names `partiton`")]
fn two_broken(w: &mut Steps) {
    let shared = STORAGE.replace(
        "data_dir = \"/data\"
",
        "",
    );
    let target = with(
        with(valid(), "storage", &shared),
        "compute_1",
        &format!("{COMPUTE}partiton = 1\n"),
    );
    w.defects = check_target(&target, None);
}

#[then("every defect is listed, each naming its instance and setting")]
fn both_named(w: &mut Steps) {
    says(
        w,
        &[
            "compute_1",
            "compute.partiton",
            "storage_1",
            "storage.data_dir",
            "storage_2",
        ],
    );
}

#[when("`storage_1` and `storage_2` both hold partition 2")]
fn held_twice(w: &mut Steps) {
    let target = with(
        with(
            valid(),
            "storage_1",
            "[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.2:7000\"\n",
        ),
        "storage_2",
        "[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.4:7000\"\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the revision is refused naming partition 2 and both instances")]
fn names_both(w: &mut Steps) {
    says(w, &["partition 2", "storage_1", "storage_2"]);
}

#[when("`partition_count` is 4 and the storage instances hold partitions 1 to 3")]
fn uncovered(w: &mut Steps) {
    let frontend = FRONTEND.replace("partition_count = 2", "partition_count = 4");
    let storage = with(
        with(valid(), "frontend", &frontend),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.2:7000\"\n",
    );
    let target = with(
        storage,
        "storage_2",
        "[storage]\npartition_first = 3\nrole_address = \"tcp://10.0.0.4:7000\"\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the revision is refused naming partition 4")]
fn names_partition(w: &mut Steps) {
    says(w, &["partition 4"]);
    assert_eq!(w.defects.len(), 1, "{}", listing(&w.defects));
}

#[when("`storage_1`'s second partition derives the address `compute_1` listens at")]
fn colliding(w: &mut Steps) {
    let frontend = FRONTEND.replace("partition_count = 2", "partition_count = 3");
    let storage = with(
        with(valid(), "frontend", &frontend),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.3:7081\"\n",
    );
    let target = with(
        with(
            storage,
            "storage_2",
            "[storage]\npartition_first = 3\nrole_address = \"tcp://10.0.0.4:7000\"\n",
        ),
        "compute_1",
        COMPUTE,
    );
    w.defects = check_target(&target, None);
}

#[then("the revision is refused naming the address and both instances")]
fn names_address(w: &mut Steps) {
    says(w, &["tcp://10.0.0.3:7082", "compute_1", "storage_1"]);
}

#[when("a revision names a `cluster.id` other than the active revision's")]
fn changed_identity(w: &mut Steps) {
    let replaced = |text: &str| text.replace("id = 7", "id = 8");
    let target = with(
        with(
            with(valid(), "frontend", &replaced(FRONTEND)),
            "storage",
            &replaced(STORAGE),
        ),
        "compute_1",
        &replaced(COMPUTE),
    );
    w.defects = check_target(&target, Some(&valid()));
}

#[then("it is refused naming both values")]
fn names_ids(w: &mut Steps) {
    says(w, &["cluster.id", "holds 8", "holds 7"]);
}

#[when("the central file does not exist")]
fn no_central(w: &mut Steps) -> TestResult {
    let path = folder(w)?.join("absent.toml");
    w.defects = read_central(&path).err().context("a refusal")?;
    Ok(())
}

#[then("the error names the setting and the path it looked at")]
fn names_path(w: &mut Steps) -> TestResult {
    let looked = w.folder.as_ref().context("a folder")?.join("absent.toml");
    says(w, &["config.targets_file", &looked.display().to_string()]);
    Ok(())
}

#[when("the central file holds no target, or a target with no file")]
fn declares_nothing(w: &mut Steps) -> TestResult {
    let path = folder(w)?;
    let none = path.join("none.toml");
    std::fs::write(&none, "# nothing\n")?;
    w.defects = read_central(&none).err().context("a refusal")?;
    let hollow = path.join("hollow.toml");
    std::fs::write(&hollow, "[ci]\n")?;
    w.defects
        .extend(read_central(&hollow).err().context("a refusal")?);
    Ok(())
}

#[then(
    "the error names the central file and says what it must declare, with the target when one is empty"
)]
fn names_declaration(w: &mut Steps) {
    says(
        w,
        &[
            "none.toml",
            "declares no target",
            "hollow.toml",
            "target ci",
            "declares no file",
            "must declare",
        ],
    );
}

#[when("target `ci` declares `storage_1 = \"ci_storage_1.toml\"` and no such file exists")]
fn absent_file(w: &mut Steps) -> TestResult {
    let path = folder(w)?;
    let central = path.join("targets.toml");
    std::fs::write(&central, "[ci]\nstorage_1 = \"ci_storage_1.toml\"\n")?;
    w.defects = read_central(&central).err().context("a refusal")?;
    Ok(())
}

#[then(
    "the error names the target, the instance, the value as written and the path it resolved to"
)]
fn names_resolved(w: &mut Steps) -> TestResult {
    let resolved = w
        .folder
        .as_ref()
        .context("a folder")?
        .join("ci_storage_1.toml");
    says(
        w,
        &[
            "target ci",
            "instance storage_1",
            "ci_storage_1.toml",
            &resolved.display().to_string(),
        ],
    );
    Ok(())
}

#[when("two declared files are absent and a third is malformed")]
fn three_wrong(w: &mut Steps) -> TestResult {
    let central = write_out(w, &with(valid(), "compute_1", "[broken\n"))?;
    let path = w.folder.clone().context("a folder")?;
    std::fs::remove_file(path.join("ci_storage_1.toml"))?;
    std::fs::remove_file(path.join("ci_storage_2.toml"))?;
    w.report = check_central(&central);
    w.defects = w.report.defects().to_vec();
    Ok(())
}

#[then("the check lists all three")]
fn lists_three(w: &mut Steps) {
    says(
        w,
        &[
            "ci_storage_1.toml",
            "ci_storage_2.toml",
            "ci_compute_1.toml",
        ],
    );
}

#[when("an administrator checks settings that fail")]
fn checks_failing(w: &mut Steps) -> TestResult {
    let central = write_out(
        w,
        &with(
            valid(),
            "storage",
            &format!("{STORAGE}partiton_count = 4\n"),
        ),
    )?;
    w.report = check_central(&central);
    Ok(())
}

#[then("the report lists the defects, and does not pass")]
fn fails(w: &mut Steps) {
    assert!(!w.report.passes());
    assert!(
        w.report
            .defects()
            .iter()
            .any(|defect| defect.to_string().contains("partiton_count"))
    );
}

#[when("an administrator checks valid settings")]
fn checks_valid(w: &mut Steps) -> TestResult {
    let central = write_out(w, &valid())?;
    w.report = check_central(&central);
    Ok(())
}

#[then("the report lists each target with its instances, and passes")]
fn passes(w: &mut Steps) {
    assert!(w.report.passes(), "{}", listing(w.report.defects()));
    assert_eq!(
        w.report.targets(),
        [(
            "ci".to_string(),
            vec![
                "compute_1".to_string(),
                "frontend".to_string(),
                "storage_1".to_string(),
                "storage_2".to_string()
            ]
        )]
    );
}

#[when("the directory holds `ci_storge_1.toml`, which no target declares")]
fn stray(w: &mut Steps) -> TestResult {
    let central = write_out(w, &valid())?;
    let path = w.folder.clone().context("a folder")?;
    std::fs::write(path.join("ci_storge_1.toml"), "")?;
    w.report = check_central(&central);
    Ok(())
}

#[then("the check warns naming the file, and still passes")]
fn warned(w: &mut Steps) {
    assert!(w.report.passes());
    assert!(
        w.report
            .warnings()
            .iter()
            .any(|warning| warning.contains("ci_storge_1.toml"))
    );
}

#[when("`ci_storage_1.toml` sets `storage.partition_count = 0`")]
fn zero_partitions(w: &mut Steps) {
    let target = with(
        valid(),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 0\nrole_address = \"tcp://10.0.0.2:7000\"\n",
    );
    w.defects = check_target(&target, None);
}

#[then(
    "the entry names the file, its line, the target, the instance, the setting, the value and the bound"
)]
fn entry_complete(w: &mut Steps) {
    says(
        w,
        &[
            "ci_storage_1.toml:3",
            "target ci",
            "instance storage_1",
            "setting storage.partition_count",
            "it holds 0",
            "at least 1",
        ],
    );
}

#[when("`ci_storage_1.toml` sets `storage.scan_stall_ms = 0`")]
fn zero_stall(w: &mut Steps) {
    let target = with(
        valid(),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.2:7000\"\nscan_stall_ms = 0\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet")]
fn stall_entry(w: &mut Steps) {
    says(
        w,
        &["setting storage.scan_stall_ms", "it holds 0", "at least 1"],
    );
}

#[when("`ci_storage_1.toml` sets `storage.scan_stall_ms = 30001`")]
fn long_stall(w: &mut Steps) {
    let target = with(
        valid(),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.2:7000\"\nscan_stall_ms = 30001\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the entry names `storage.scan_stall_ms`, the value 30001 and the bound it must meet")]
fn long_stall_entry(w: &mut Steps) {
    says(
        w,
        &[
            "setting storage.scan_stall_ms",
            "it holds 30001",
            "at most 30000",
        ],
    );
}

#[when("`ci_frontend.toml` sets `storage.scan_stall_ms = 0`")]
fn zero_frontend_stall(w: &mut Steps) {
    let frontend = format!("{FRONTEND}[storage]\nscan_stall_ms = 0\n");
    let target = with(valid(), "frontend", &frontend);
    w.defects = check_target(&target, None);
}

#[when("`ci_storage_1.toml` sets `storage.read_retain_ms = 0`")]
fn zero_retention(w: &mut Steps) {
    let target = with(
        valid(),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.2:7000\"\nread_retain_ms = 0\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the entry names `storage.read_retain_ms`, the value 0 and the bound it must meet")]
fn retention_entry(w: &mut Steps) {
    says(
        w,
        &["setting storage.read_retain_ms", "it holds 0", "at least 1"],
    );
}

#[when("`ci_storage_1.toml` sets `storage.held_bytes = 0`")]
fn zero_parked_bytes(w: &mut Steps) {
    let target = with(
        valid(),
        "storage_1",
        "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://10.0.0.2:7000\"\nheld_bytes = 0\n",
    );
    w.defects = check_target(&target, None);
}

#[then("the entry names `storage.held_bytes`, the value 0 and the bound it must meet")]
fn parked_bytes_entry(w: &mut Steps) {
    says(
        w,
        &["setting storage.held_bytes", "it holds 0", "at least 1"],
    );
}

#[when("`ci_frontend.toml` sets `endpoint.held_bytes = 0`")]
fn zero_held_bytes(w: &mut Steps) {
    let frontend = format!("{FRONTEND}[endpoint]\nplaintext = true\nheld_bytes = 0\n");
    let target = with(valid(), "frontend", &frontend);
    w.defects = check_target(&target, None);
}

#[then("the entry names `endpoint.held_bytes`, the value 0 and the bound it must meet")]
fn held_bytes_entry(w: &mut Steps) {
    says(
        w,
        &["setting endpoint.held_bytes", "it holds 0", "at least 1"],
    );
}

#[when("`ci_storage.toml` holds `partiton_count = 4` under `[storage]`")]
fn misspelled(w: &mut Steps) {
    let target = with(valid(), "storage", "[storage]\n\npartiton_count = 4\n");
    w.defects = check_target(&target, None);
}

#[then(
    "the entry names the file, the line, `storage.partiton_count`, and says no such setting exists"
)]
fn misspelling_named(w: &mut Steps) {
    says(
        w,
        &[
            "ci_storage.toml:3",
            "storage.partiton_count",
            "no such setting exists",
        ],
    );
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
