/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Helm chart `deploy/helm/ultragraphdb` as the Helm of rules_helm's toolchain renders it, bound to `features/`: the
//! rendered objects, one JSON document per line (`BUILD.bazel`), and the chart's own files. No process, cluster or network.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cucumber::{World, then, when};
use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The roles' user, the glibc base's nonroot user (kubernetes-deployment, "A pod runs unprivileged").
const ROLE_USER: u64 = 65_532;

/// The config service's user, the scratch base's `nobody` (config-service, "One config service holds every role's settings").
const CONFIG_USER: u64 = 65_534;

/// The Secret holding the bootstrap administrator's password, which the chart names in `templates/secrets.yaml`.
const BOOTSTRAP_SECRET: &str = "ultragraphdb-bootstrap-secret";

/// The Secret holding the role secret.
const ROLE_SECRET: &str = "ultragraphdb-role-secret";

/// The four variables a role takes from its environment when the config service is reached over clear text
/// (config-service, "A role starts from five bootstrap variables": the fifth, the CA file, only with `https`).
const BOOTSTRAP_VARIABLES: [&str; 4] = [
    "ULTRAGRAPHDB_CONFIG_URL",
    "ULTRAGRAPHDB_CONFIG_TARGET",
    "ULTRAGRAPHDB_CONFIG_INSTANCE",
    "ULTRAGRAPHDB_CONFIG_KEY_FILE",
];

/// A run of at least this many characters of the base64 alphabet, hexadecimal among them, reads as a key or a secret: the
/// chart draws its own as 64 alphanumerics.
const SECRET_RUN: usize = 32;

#[derive(Debug, Default, World)]
struct Steps {
    /// The release rendered and its objects.
    release: Release,
    /// The pods a step inspects: each workload's name and its pod template's spec.
    pods: Vec<(String, Value)>,
    /// The chart's files searched, each path and text.
    files: Vec<(PathBuf, String)>,
}

/// A rendered release: its name, namespace, domain and objects.
#[derive(Debug, Default)]
struct Release {
    namespace: String,
    domain: String,
    objects: Vec<Value>,
}

/// The test's package directory, in Bazel's runfiles or Cargo's tree.
fn package() -> TestResult<PathBuf> {
    let features = ultragraphdb_tool_bdd::features_dir()?;
    Ok(features
        .parent()
        .context("the package directory")?
        .to_path_buf())
}

/// The release `name` rendered into `namespace` under `domain`: the objects of `rendered_<name>.jsonl`.
fn rendered(file: &str, namespace: String, domain: String) -> TestResult<Release> {
    let path = package()?.join(format!("rendered_{file}.jsonl"));
    let text = std::fs::read_to_string(&path).context(&path.display().to_string())?;
    let objects = serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .filter(|object| !matches!(object, Ok(Value::Null)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Release {
        namespace,
        domain,
        objects,
    })
}

/// The release `scripts/cluster_local.sh` renders: `ultragraphdb` in its namespace, under the chart's default domain.
fn default_release() -> TestResult<Release> {
    rendered(
        "kube",
        "ultragraphdb".to_string(),
        "cluster.local".to_string(),
    )
}

/// The text at `pointer` of `value`.
fn text<'a>(value: &'a Value, pointer: &str) -> TestResult<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| TestError::new(format!("{pointer} in {value}")))
}

/// The array at `pointer` of `value`, empty when absent.
fn list<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

impl Release {
    /// The object of `kind` named `name`.
    fn object(&self, kind: &str, name: &str) -> TestResult<&Value> {
        self.objects
            .iter()
            .find(|object| {
                object.pointer("/kind").and_then(Value::as_str) == Some(kind)
                    && object.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
            })
            .ok_or_else(|| TestError::new(format!("no {kind} {name} in the release")))
    }

    /// Every workload's name and pod spec: the Deployments' and StatefulSets'.
    fn pods(&self) -> TestResult<Vec<(String, Value)>> {
        self.objects
            .iter()
            .filter(|object| {
                matches!(
                    object.pointer("/kind").and_then(Value::as_str),
                    Some("Deployment" | "StatefulSet")
                )
            })
            .map(|object| {
                let name = text(object, "/metadata/name")?.to_string();
                let spec = object
                    .pointer("/spec/template/spec")
                    .context("a pod template")?
                    .clone();
                Ok((name, spec))
            })
            .collect()
    }

    /// The profile file `file` the ConfigMap carries.
    fn profile(&self, file: &str) -> TestResult<&str> {
        let map = self.object("ConfigMap", "ultragraphdb-profile")?;
        text(map, &format!("/data/{}", file.replace('/', "~1")))
    }
}

/// The value of `key` in the table `table` of `toml`, a settings file of the subset the roles read.
fn setting<'a>(toml: &'a str, table: &str, key: &str) -> TestResult<&'a str> {
    let mut current = "";
    toml.lines()
        .find_map(|line| {
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                current = name;
                return None;
            }
            let (name, value) = line.split_once('=')?;
            (current == table && name.trim() == key).then(|| value.trim().trim_matches('"'))
        })
        .ok_or_else(|| TestError::new(format!("{table}.{key} in\n{toml}")))
}

/// The port of `address`, `tcp://host:port`.
fn port_of(address: &str) -> TestResult<u64> {
    Ok(address.rsplit_once(':').context("a port")?.1.parse()?)
}

/// The names of the Secrets `pod`'s volumes project or mount.
fn secrets_of(pod: &Value) -> BTreeSet<String> {
    list(pod, "/volumes")
        .iter()
        .flat_map(|volume| {
            let projected = list(volume, "/projected/sources")
                .iter()
                .filter_map(|source| source.pointer("/secret/name").and_then(Value::as_str));
            let whole = volume.pointer("/secret/secretName").and_then(Value::as_str);
            projected
                .chain(whole)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The one container of `pod`.
fn container(pod: &Value) -> TestResult<&Value> {
    match list(pod, "/containers") {
        [one] => Ok(one),
        many => Err(TestError::new(format!("{} containers", many.len()))),
    }
}

/// The ports `container` declares.
fn ports_of(container: &Value) -> BTreeSet<u64> {
    list(container, "/ports")
        .iter()
        .filter_map(|port| port.pointer("/containerPort").and_then(Value::as_u64))
        .collect()
}

/// Whether `path` lies at or below `directory`, component by component.
fn below(path: &str, directory: &str) -> bool {
    Path::new(path).starts_with(directory)
}

/// Where the volume claim `claim` is mounted in `container`.
fn mounted_at<'a>(container: &'a Value, claim: &str) -> TestResult<&'a str> {
    list(container, "/volumeMounts")
        .iter()
        .find(|mount| mount.pointer("/name").and_then(Value::as_str) == Some(claim))
        .and_then(|mount| mount.pointer("/mountPath").and_then(Value::as_str))
        .ok_or_else(|| TestError::new(format!("no mount of {claim} in {container}")))
}

#[when("any role's pod is inspected")]
fn every_pod(w: &mut Steps) -> TestResult {
    w.release = default_release()?;
    w.pods = w.release.pods()?;
    Ok(())
}

#[then(
    "it runs as its image's user, 65532 for a role and 65534 for the config service, with a read-only root filesystem, no capability and no privilege escalation"
)]
fn unprivileged(w: &mut Steps) -> TestResult {
    let names: BTreeSet<&str> = w.pods.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        BTreeSet::from(["compute", "config", "frontend", "storage"])
    );
    for (name, pod) in &w.pods {
        let user = if name == "config" {
            CONFIG_USER
        } else {
            ROLE_USER
        };
        assert_eq!(
            pod.pointer("/securityContext/runAsUser"),
            Some(&Value::from(user)),
            "{name}"
        );
        assert_eq!(
            pod.pointer("/securityContext/runAsNonRoot"),
            Some(&Value::Bool(true)),
            "{name}"
        );
        for one in list(pod, "/containers") {
            let security = one
                .pointer("/securityContext")
                .context("a container's security")?;
            assert_eq!(
                security.pointer("/readOnlyRootFilesystem"),
                Some(&Value::Bool(true)),
                "{name}"
            );
            assert_eq!(
                security.pointer("/allowPrivilegeEscalation"),
                Some(&Value::Bool(false)),
                "{name}"
            );
            assert_eq!(
                security.pointer("/capabilities/drop"),
                Some(&Value::from(vec!["ALL"])),
                "{name}"
            );
            assert!(
                security.pointer("/capabilities/add").is_none(),
                "{name} adds a capability"
            );
            assert!(
                security.pointer("/privileged").and_then(Value::as_bool) != Some(true),
                "{name}"
            );
        }
    }
    Ok(())
}

/// Holds the pod of the workload `name` in the default release.
fn one_pod(w: &mut Steps, name: &str) -> TestResult {
    w.release = default_release()?;
    w.pods = w
        .release
        .pods()?
        .into_iter()
        .filter(|(workload, _)| workload == name)
        .collect();
    assert_eq!(w.pods.len(), 1, "one {name} workload");
    Ok(())
}

#[when("the frontend's pod is inspected")]
fn frontend_pod(w: &mut Steps) -> TestResult {
    one_pod(w, "frontend")
}

#[then("it mounts the role secret and no bootstrap secret, and holds no volume claim")]
fn no_bootstrap(w: &mut Steps) -> TestResult {
    let (_, pod) = w.pods.first().context("the frontend's pod")?;
    let secrets = secrets_of(pod);
    assert!(secrets.contains(ROLE_SECRET), "{secrets:?}");
    assert!(!secrets.contains(BOOTSTRAP_SECRET), "{secrets:?}");
    let workload = w.release.object("Deployment", "frontend")?;
    assert!(workload.pointer("/spec/volumeClaimTemplates").is_none());
    for volume in list(pod, "/volumes") {
        assert!(
            volume.pointer("/persistentVolumeClaim").is_none(),
            "{volume}"
        );
    }
    Ok(())
}

/// Every file under `directory`, its path and text.
fn files_under(directory: &Path) -> TestResult<Vec<(PathBuf, String)>> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    // Each turn takes one directory off the list, which a chart's few directories bound.
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let text = std::fs::read_to_string(&path).context(&path.display().to_string())?;
                found.push((path, text));
            }
        }
    }
    Ok(found)
}

#[when("the files under `deploy/helm/` are searched")]
fn search(w: &mut Steps) -> TestResult {
    let deploy = package()?.join("../../deploy/helm");
    w.files = files_under(&deploy)?;
    // The search reaches the chart itself, its Secrets' template among them, so it passes by reading and not by finding
    // nothing to read.
    let templates = deploy.join("ultragraphdb/templates/secrets.yaml");
    assert!(
        w.files.iter().any(|(path, _)| path == &templates),
        "{} among {} files",
        templates.display(),
        w.files.len()
    );
    w.release = default_release()?;
    Ok(())
}

/// The runs of `text` that read as a key or a secret: at least [`SECRET_RUN`] characters of the base64 alphabet holding a
/// digit, a lowercase and an uppercase letter, as a random draw of that length does but for a chance below 10^-7, or at least
/// as many hexadecimal digits. A path or a name of the chart is a run of the alphabet too, and holds no digit and no capital.
fn secret_like(text: &str) -> Vec<&str> {
    let base64 = text
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='))
        .filter(|run| {
            run.len() >= SECRET_RUN
                && run.contains(|c: char| c.is_ascii_digit())
                && run.contains(|c: char| c.is_ascii_lowercase())
                && run.contains(|c: char| c.is_ascii_uppercase())
        });
    let hex = text
        .split(|c: char| !c.is_ascii_hexdigit())
        .filter(|run| run.len() >= SECRET_RUN);
    base64.chain(hex).collect()
}

#[then("none holds a key, a secret or a password")]
fn no_secret(w: &mut Steps) -> TestResult {
    // What the chart draws at a render: each Secret's value, encoded as the object holds it.
    let drawn: Vec<&str> = w
        .release
        .objects
        .iter()
        .filter(|object| object.pointer("/kind").and_then(Value::as_str) == Some("Secret"))
        .flat_map(|secret| secret.pointer("/data").and_then(Value::as_object))
        .flat_map(|data| data.values().filter_map(Value::as_str))
        .collect();
    assert_eq!(drawn.len(), 4, "the four Secrets' values");
    for (path, text) in &w.files {
        let file = path.display();
        assert!(!text.contains("-----BEGIN"), "{file} holds a PEM block");
        assert_eq!(secret_like(text), Vec::<&str>::new(), "{file}");
        for value in &drawn {
            assert!(!text.contains(value), "{file} holds a drawn value");
        }
    }
    Ok(())
}

#[when("the chart is rendered as the release `ultragraphdb`")]
fn release(w: &mut Steps) -> TestResult {
    w.release = default_release()?;
    w.pods = w.release.pods()?;
    Ok(())
}

/// The headless Service selecting the pods of `name`, publishing not-ready addresses.
fn headless(release: &Release, name: &str) -> TestResult {
    let service = release.object("Service", name)?;
    assert_eq!(text(service, "/spec/clusterIP")?, "None", "{name}");
    assert_eq!(
        service.pointer("/spec/publishNotReadyAddresses"),
        Some(&Value::Bool(true)),
        "{name}"
    );
    assert_eq!(
        text(service, "/spec/selector/app.kubernetes.io~1name")?,
        name
    );
    Ok(())
}

#[then(
    "the config service and the storage instance are StatefulSets of one replica, each holding its data directory on a volume of its own, the frontend and compute are Deployments with no volume, each StatefulSet and compute has a headless Service publishing not-ready addresses, and the frontend's client endpoint has a Service"
)]
fn workloads(w: &mut Steps) -> TestResult {
    let release = &w.release;
    let storage = release.profile("storage.toml")?;
    let config = release.profile("kube.toml")?;
    // Each StatefulSet's data directories, as its settings name them.
    for (name, directories) in [
        ("config", vec![setting(config, "config", "data_dir")?]),
        (
            "storage",
            vec![
                setting(storage, "storage", "data_dir")?,
                setting(storage, "system", "data_dir")?,
            ],
        ),
    ] {
        let set = release.object("StatefulSet", name)?;
        assert_eq!(
            set.pointer("/spec/replicas"),
            Some(&Value::from(1)),
            "{name}"
        );
        let claims = list(set, "/spec/volumeClaimTemplates");
        let [claim] = claims else {
            return Err(TestError::new(format!("{name}: {} claims", claims.len())));
        };
        let pod = set.pointer("/spec/template/spec").context("a pod")?;
        let mount = mounted_at(container(pod)?, text(claim, "/metadata/name")?)?;
        for directory in directories {
            assert!(
                below(directory, mount),
                "{name}: {directory} outside {mount}"
            );
        }
        headless(release, name)?;
    }
    for name in ["frontend", "compute"] {
        let deployment = release.object("Deployment", name)?;
        assert_eq!(
            deployment.pointer("/spec/replicas"),
            Some(&Value::from(1)),
            "{name}"
        );
        let pod = deployment.pointer("/spec/template/spec").context("a pod")?;
        // A Secret's projection is how a pod reads a key; a volume of data would be a claim, an emptyDir or a host path.
        for volume in list(pod, "/volumes") {
            assert!(volume.pointer("/projected").is_some(), "{name}: {volume}");
        }
    }
    headless(release, "compute")?;
    let client = release.object("Service", "frontend-client")?;
    assert_eq!(
        text(client, "/spec/selector/app.kubernetes.io~1name")?,
        "frontend"
    );
    let endpoint = port_of(setting(
        release.profile("frontend.toml")?,
        "endpoint",
        "address",
    )?)?;
    let served: BTreeSet<u64> = list(client, "/spec/ports")
        .iter()
        .filter_map(|port| port.pointer("/port").and_then(Value::as_u64))
        .collect();
    assert_eq!(served, BTreeSet::from([endpoint]));
    Ok(())
}

#[then(
    "its container declares the client endpoint's port and its observability port, and no other"
)]
fn frontend_ports(w: &mut Steps) -> TestResult {
    let (_, pod) = w.pods.first().context("the frontend's pod")?;
    let frontend = w.release.profile("frontend.toml")?;
    let expected = BTreeSet::from([
        port_of(setting(frontend, "endpoint", "address")?)?,
        port_of(setting(frontend, "observability", "address")?)?,
    ]);
    assert_eq!(ports_of(container(pod)?), expected);
    Ok(())
}

#[when("the storage pod is inspected")]
fn storage_pod(w: &mut Steps) -> TestResult {
    one_pod(w, "storage")
}

#[then(
    "its Service and its container declare the port `system.role_address` names, the group's logs lie on its volume, and it alone of every pod mounts the bootstrap secret"
)]
fn storage_hosts(w: &mut Steps) -> TestResult {
    let (_, pod) = w.pods.first().context("the storage pod")?;
    let storage = w.release.profile("storage.toml")?;
    let system = port_of(setting(storage, "system", "role_address")?)?;
    let one = container(pod)?;
    assert!(ports_of(one).contains(&system), "{one}");
    let service = w.release.object("Service", "storage")?;
    assert!(
        list(service, "/spec/ports")
            .iter()
            .any(|port| port.pointer("/port").and_then(Value::as_u64) == Some(system)),
        "{service}"
    );
    let set = w.release.object("StatefulSet", "storage")?;
    let claim = text(set, "/spec/volumeClaimTemplates/0/metadata/name")?;
    let logs = setting(storage, "system", "data_dir")?;
    assert!(below(logs, mounted_at(one, claim)?), "{logs}");
    let holding: Vec<String> = w
        .release
        .pods()?
        .into_iter()
        .filter(|(_, spec)| secrets_of(spec).contains(BOOTSTRAP_SECRET))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(holding, ["storage"]);
    // The host reads the password at the file its settings name, inside the mount holding the Secret.
    let file = setting(storage, "metadata", "bootstrap_secret_file")?;
    let secrets = mounted_at(one, "secrets")?;
    assert!(below(file, secrets), "{file} outside {secrets}");
    Ok(())
}

#[when(
    expr = "the chart is rendered as the release `{word}` into the namespace `{word}` under the domain `{word}` with {int} partitions"
)]
fn rendered_as(
    w: &mut Steps,
    release: String,
    namespace: String,
    domain: String,
    _partitions: u32,
) -> TestResult {
    let file = if release == "ultragraphdb" {
        "kube".to_string()
    } else {
        release
    };
    w.release = rendered(&file, namespace, domain)?;
    Ok(())
}

#[then(
    expr = "every role's `cluster.system_address` and the host's `system.role_address` name the hosting storage pod's stable name at the system group's port, and the host holds {int} partitions"
)]
fn system_address(w: &mut Steps, partitions: u32) -> TestResult {
    let release = &w.release;
    // A StatefulSet's first pod is `<set>-0`, named under its Service as `<pod>.<service>.<namespace>.svc.<domain>`.
    let set = release.object("StatefulSet", "storage")?;
    let pod = format!("{}-0", text(set, "/metadata/name")?);
    let service = text(set, "/spec/serviceName")?;
    let port = list(release.object("Service", service)?, "/spec/ports")
        .iter()
        .find(|port| port.pointer("/name").and_then(Value::as_str) == Some("system"))
        .and_then(|port| port.pointer("/port").and_then(Value::as_u64))
        .context("the system group's port on the storage Service")?;
    let expected = format!(
        "tcp://{pod}.{service}.{}.svc.{}:{port}",
        release.namespace, release.domain
    );
    for role in ["frontend.toml", "storage.toml", "compute.toml"] {
        let file = release.profile(role)?;
        assert_eq!(
            setting(file, "cluster", "system_address")?,
            expected,
            "{role}"
        );
    }
    let storage = release.profile("storage.toml")?;
    assert_eq!(setting(storage, "system", "role_address")?, expected);
    let count = partitions.to_string();
    assert_eq!(setting(storage, "cluster", "partition_count")?, count);
    assert_eq!(setting(storage, "storage", "partition_count")?, count);
    Ok(())
}

#[when("the frontend's, storage's and compute's pods are inspected")]
fn role_pods(w: &mut Steps) -> TestResult {
    w.release = default_release()?;
    w.pods = w
        .release
        .pods()?
        .into_iter()
        .filter(|(name, _)| name != "config")
        .collect();
    assert_eq!(w.pods.len(), 3);
    Ok(())
}

#[then(
    "each one's environment holds the four bootstrap variables of a service over clear text and no other, naming the config service's Service in the release's namespace, the profile's target, the pod's own instance and the read key's file"
)]
fn bootstrap_variables(w: &mut Steps) -> TestResult {
    let release = &w.release;
    let service = release.object("Service", "config")?;
    let api = list(service, "/spec/ports")
        .first()
        .and_then(|port| port.pointer("/port").and_then(Value::as_u64))
        .context("the config service's port")?;
    let url = format!(
        "http://config.{}.svc.{}:{api}",
        release.namespace, release.domain
    );
    let targets = release.profile("targets.toml")?;
    let target = targets
        .lines()
        .find_map(|line| {
            line.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        })
        .context("the profile's target")?;
    let key = setting(release.profile("kube.toml")?, "config", "read_key_file")?;
    for (name, pod) in &w.pods {
        let env: Vec<(&str, &str)> = list(container(pod)?, "/env")
            .iter()
            .map(|variable| Ok((text(variable, "/name")?, text(variable, "/value")?)))
            .collect::<TestResult<_>>()?;
        let names: BTreeSet<&str> = env.iter().map(|(variable, _)| *variable).collect();
        assert_eq!(names, BTreeSet::from(BOOTSTRAP_VARIABLES), "{name}");
        let value = |variable: &str| {
            env.iter()
                .find(|(held, _)| *held == variable)
                .map(|(_, value)| *value)
        };
        assert_eq!(
            value("ULTRAGRAPHDB_CONFIG_URL"),
            Some(url.as_str()),
            "{name}"
        );
        assert_eq!(value("ULTRAGRAPHDB_CONFIG_TARGET"), Some(target), "{name}");
        assert_eq!(
            value("ULTRAGRAPHDB_CONFIG_INSTANCE"),
            Some(name.as_str()),
            "{name}"
        );
        assert_eq!(value("ULTRAGRAPHDB_CONFIG_KEY_FILE"), Some(key), "{name}");
    }
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
