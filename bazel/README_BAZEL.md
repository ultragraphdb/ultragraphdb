# Release binaries and OCI images

`bazel/` holds the rules that turn a Rust crate into a shipped artifact: a release binary, and a multi-arch OCI image
around it. Everything here is built by Bazel, from sources and pinned inputs, with no Dockerfile and no Docker daemon.

| Path                                         | What it provides                                                                    |
|----------------------------------------------|-------------------------------------------------------------------------------------|
| `bazel/binary.bzl`                           | `static_binary`, every release binary's rule                                       |
| `bazel/container/img.bzl`                    | `scratch_multiarch_img`, `glibc_multiarch_img`, `sha256_tag`, and the platform lists |
| `bazel/oci_images/scrach_base/`              | The scratch base image: CA bundle and the `nobody` user, nothing else               |
| `bazel/oci_images/glibc_base/`               | The glibc base image, built from Wolfi packages by apko                             |
| `bazel/oci_images/seccomp/`                  | The seccomp profile the role images ship with: Docker's default plus io_uring       |
| `//:release` (root `BUILD.bazel`)            | The `config_setting` that is true under `-c opt`                                    |

## Which image a binary ships in

| Binary                                   | Linked against            | Image                   | Base                 |
|------------------------------------------|---------------------------|-------------------------|----------------------|
| The server's roles                       | glibc, with mimalloc      | `glibc_multiarch_img`   | `glibc_base` (Wolfi) |
| The development image: the supervisor, the config service and the three roles | glibc | `development_multiarch` in `//ultragraphdb/ultragraphdb_supervisor` | `glibc_base` (Wolfi) |
| Tools, benchmarks, anything else         | musl, statically          | `scratch_multiarch_img` | `scrach_base`        |

The server ships on glibc because musl with mimalloc measured about 18% slower per core (D7,
`openspec/notes/UltraGraphDB Distribution Architecture.md`). A tool gains nothing from that and ships as one static
file on scratch, which has no loader, no libc and no shell to attack.

Both image macros build for `linux/amd64` and `linux/arm64` in one index. The platform lists are `GNU_PLATFORMS` and
`MUSL_PLATFORMS` in `img.bzl`; each triple there must also be in `RUST_PLATFORMS` in `MODULE.bazel`, which is the list
the crate graph is resolved for.

## Release binaries: `static_binary`

Every binary in the repository is declared with `static_binary` from `//bazel:binary.bzl`, so all of them are built
with the same release policy. Under `-c opt` (the `//:release` setting) the binary's crate gets `RELEASE_FLAGS`:

```
-Copt-level=3  -Ccodegen-units=1  -Cstrip=symbols  -Cpanic=unwind
```

One codegen unit lets inlining cross the whole crate, and the full symbol strip removes what Bazel's own
`--codegen=strip=debuginfo` leaves; together they take a binary from 17 MB to 3.2 MB. `-Cpanic=unwind` keeps `Drop`
running when a thread panics. Without `-c opt` the binary is an ordinary debug build. `binary.bzl` records the flags
that were measured and rejected, LTO among them, and why.

```python
load("//bazel:binary.bzl", "static_binary")

static_binary(
    name = "wal_bench",
    srcs = ["src/bin/wal_bench.rs"],
    main = "src/bin/wal_bench.rs",
    deps = [":ultragraphdb_tool_bench"] + all_crate_deps(normal = True),
)
```

| Argument        | Meaning                                                                                                     |
|-----------------|-------------------------------------------------------------------------------------------------------------|
| `srcs`          | Every source, `main` included                                                                               |
| `main`          | The binary's crate root, `src/main.rs` by default                                                           |
| `library`       | Optional. Names a library built from every source but `main`, for a crate that others `use`; the binary depends on it |
| `crate_root`    | The library's crate root, `src/lib.rs` by default; read only with `library`                                |
| `release_flags` | Replaces `RELEASE_FLAGS` under `-c opt`. Write the complete list: the default is replaced, never extended   |

`wal_bench_profile` in `ultragraphdb_tool/ultragraphdb_tool_bench/BUILD.bazel` is the worked example of
`release_flags`: the release optimisation with line tables and frame pointers kept, for `perf`.

The platform decides the linking. Built for a musl triple, the binary is static: no `PT_INTERP` segment and no
`DT_NEEDED` entry, so the kernel runs it with nothing else in the image. Built for a gnu triple, it links glibc
dynamically, and the flags apply all the same.

```bash
# Static, for the scratch image or any Linux host.
bazel build -c opt //ultragraphdb_tool/ultragraphdb_tool_bench:wal_bench \
    --platforms=@rules_rs//rs/platforms:x86_64-unknown-linux-musl

# On this machine, to run it.
bazel run -c opt //ultragraphdb_tool/ultragraphdb_tool_bench:wal_bench -- --dir /tmp/wal --rate 1000
```

## Images of a binary

```python
load("//bazel/container:img.bzl", "glibc_multiarch_img", "scratch_multiarch_img", "sha256_tag")

scratch_multiarch_img(
    name = "wal_bench_multiarch",
    binary = ":wal_bench",
)

glibc_multiarch_img(
    name = "wal_bench_glibc_multiarch",
    binary = ":wal_bench",
)
```

Each macro takes a binary target and declares three targets:

| Target              | What it is                                                                                   |
|---------------------|----------------------------------------------------------------------------------------------|
| `<name>_layer`      | One layer holding the binary at `/<binary target name>`, e.g. `/wal_bench`                    |
| `<name>_manifest`   | The base plus that layer, with the binary as the entrypoint. Tagged `manual`                  |
| `<name>`            | The multi-arch index, one manifest per platform. This is the target to build, load and push  |

The index transitions the binary to each platform, so the binary is a plain target and is compiled once per
architecture. The manifest is tagged `manual` because both bases hold Linux images only: it builds through the index's
transition, never alone for the host. Each image runs as its base's unprivileged user: `nobody` (65534) on scratch,
`nonroot` (65532) on glibc. A directory the binary writes, such as a mounted volume, must be writable by that user.

**Build images with `-c opt`.** Without it the image carries a debug binary about five times the size.

```bash
bazel build -c opt //ultragraphdb_tool/ultragraphdb_tool_bench:wal_bench_multiarch
```

### Run it locally

`image_load` from rules_img loads an image into the local Docker daemon:

```python
load("@rules_img//img:load.bzl", "image_load")

image_load(
    name = "wal_bench_glibc_load",
    image = ":wal_bench_glibc_multiarch",
    tag = "ultragraphdb/wal_bench:glibc",
)
```

```bash
bazel run -c opt //ultragraphdb_tool/ultragraphdb_tool_bench:wal_bench_glibc_load
docker run --rm --security-opt seccomp=unconfined ultragraphdb/wal_bench:glibc --dir /tmp/wal --rate 1000 --seconds 5
```

A binary that does I/O through compio uses io_uring on Linux, and Docker's default seccomp profile refuses its system
calls: without `--security-opt seccomp=unconfined` the run above stops at its first I/O with
`Operation not permitted (os error 1)`. The roles probe the ring at start instead and fall back to compio's polling driver,
logging which they run on; `--security-opt seccomp=bazel/oci_images/seccomp/ultragraphdb.json` keeps the rest of Docker's
default profile and allows io_uring.

### Tag it by content

`sha256_tag` writes the first eight hex characters of the index's SHA-256, its OCI digest, to `<name>.txt`. The same
image always gets the same tag.

```python
sha256_tag(
    name = "wal_bench_tag",
    image = ":wal_bench_multiarch",
)
```

### Push it

`image_push` from rules_img pushes an index with every tag in `tag_list`, all naming one digest. Credentials come from
the Docker configuration, `~/.docker/config.json` and its credential helper, so run `docker login <registry>` once.

```python
load("@rules_img//img:push.bzl", "image_push")

image_push(
    name = "wal_bench_push",
    image = ":wal_bench_multiarch",
    registry = "index.docker.io",
    repository = "<org>/wal_bench",
    tag_list = ["latest"],
)
```

```bash
bazel run -c opt //ultragraphdb_tool/ultragraphdb_tool_bench:wal_bench_push
```

Push every tag from the one target, as `tag_list` does, and never retag with `docker buildx imagetools create`: it
rebuilds the index rather than copying it. `bazel/oci_images/scrach_base/README.md` describes the failure.

## The scratch base: `//bazel/oci_images/scrach_base`

The base of every `scratch_multiarch_img`, built from three files in two layers:

| Layer          | File                                    | Source                                                                  |
|----------------|-----------------------------------------|-------------------------------------------------------------------------|
| `user_layer`   | `/etc/passwd`, `/etc/group`             | `passwd` and `group` in the package: the user and group `nobody`, 65534 |
| `cacert_layer` | `/etc/ssl/certs/ca-certificates.crt`    | `@mozilla_cacert`, Mozilla's CA bundle as curl publishes it              |

Every file is owned by root with mode 0644 and dated 1970, so the image is reproducible: the same inputs give the same
digest, and the layers are byte-identical on both architectures. The manifest has no base, which is what scratch is.

A static binary carries no trust store, and `ultragraphdb_transport_tls` reads `/etc/ssl/certs/ca-certificates.crt`
(or the file `SSL_CERT_FILE` names), so the bundle is what lets a tool on scratch verify TLS. The manifest's `user` is
`65534`, so the base and every image built on it run as `nobody`, which `/etc/passwd` names. An image that must run
otherwise sets its own `user` on its `image_manifest`, and `docker run --user` overrides it at run time.

`scratch_multiarch_img` builds on the target directly. No registry is involved, and the base changes only when this
package or the pinned bundle does.

### Update the CA bundle

curl republishes Mozilla's bundle under a dated name with its SHA-256 beside it. The pin is the `mozilla_cacert`
`http_file` in `MODULE.bazel`, and the date and the hash change together:

```bash
curl -sSfL https://curl.se/ca/cacert-<date>.pem.sha256   # the hash for the new date
```

Put the new date in `urls` and the hash in `sha256`, then rebuild. A wrong hash fails the fetch.

### Publish it

The image is also published, for consumers outside this repository, as `hansenmarvin/rust-scratch` on Docker Hub:

```bash
bazel run //bazel/oci_images/scrach_base:push
```

`tag_list` names the version tag, which follows the Rust version (`RUST_DEFAULT_VERSION` in `MODULE.bazel`), and
`latest`. Update the version tag before pushing. Verifying, signing and attesting the published image are in
`bazel/oci_images/scrach_base/README.md`.

## The glibc base: `//bazel/oci_images/glibc_base`

The base of every `glibc_multiarch_img`: Wolfi's `glibc`, its loader `ld-linux`, `libgcc` for unwinding, the CA bundle
and the user `nonroot` (65532), with no shell and no package manager. These are the packages of Chainguard's
`glibc-dynamic`.

| File / target        | Role                                                                                   |
|----------------------|----------------------------------------------------------------------------------------|
| `apko.yaml`          | The packages, the account and the two architectures                                    |
| `apko.lock.json`     | Every package resolved to an exact version and checksum                                |
| `:glibc_base`        | The apko image, built from the lock file alone                                         |
| `:glibc_base_index`  | The same image as a rules_img index, which `GLIBC_BASE` in `img.bzl` names             |
| `:lock`              | Resolves `apko.yaml` again and rewrites `apko.lock.json`                              |

The build reads only the lock file, through the `glibc_base_lock` repository that `apko.translate_lock` in
`MODULE.bazel` declares, so an image is reproducible until the lock changes. To take newer packages, or after editing
`apko.yaml`:

```bash
bazel run //bazel/oci_images/glibc_base:lock
```

then review and commit the changed `apko.lock.json`.

## The seccomp profile: `//bazel/oci_images/seccomp`

`ultragraphdb.json` is Docker's default profile, `seccomp/v0.3.0` of `github.com/moby/profiles` (commit
`4b8214fc4971e06172938a1f1554ccff6647ee80`), with one rule added after its first, allowing `io_uring_setup`, `io_uring_enter`
and `io_uring_register`. Docker's own default refuses those three, so a role under it runs on the polling driver. To take a
newer default, copy `seccomp/default.json` of the new tag and add the same rule after its first.

A runtime reads a profile from its host, so no image holds it. Each image's `push` publishes it as a referrer of the image:
`seccomp_referrer` in `//bazel/container:img.bzl` declares an OCI artifact of type
`application/vnd.ultragraphdb.seccomp-profile.v1+json` whose one layer is `ultragraphdb.json` (`:profile_layer`) and whose
subject is the image index, and the push's `referrers` sends it to the image's repository in the same `bazel run`, bound to
the image's digest. A registry serving the OCI referrers API lists it there; one that does not receives the referrers tag
`sha256-<index digest>`. Either way:

```bash
oras discover --artifact-type application/vnd.ultragraphdb.seccomp-profile.v1+json <repo>:<tag>
oras pull <repo>@<artifact digest>
```

## The role images and the development image

Each role's package, `//ultragraphdb/ultragraphdb_frontend`, `_compute` and `_storage`, declares `<role>_multiarch`, its
`<role>_tag`, `push` and `load`; `//ultragraphdb/ultragraphdb_config` declares the config service's scratch image the same
way. `//ultragraphdb/ultragraphdb_supervisor` declares the development image, `development_multiarch`, with
`development_tag`, `push` and `development_load`: the supervisor, the config service and the three roles at `/`, where the
supervisor finds the binaries beside it, the shipped profiles under `/etc/ultragraphdb/profiles/`, and `/data` and
`/run/ultragraph` owned by 65532 at mode 0700. It runs `ultragraphdb_supervisor --config /etc/ultragraphdb/profiles/dev.toml`
as 65532, the client endpoint at 7080. The supervisor draws the config service's keys, the role secret and the bootstrap
administrator's password under `/data/config/` at the first start; the password is `bootstrap.secret`:

```bash
bazel run -c opt //ultragraphdb/ultragraphdb_supervisor:development_load -- --platform linux/arm64
docker run -d --name ultragraphdb -p 7080:7080 -v ultragraphdb:/data \
    --security-opt seccomp=bazel/oci_images/seccomp/ultragraphdb.json hansenmarvin/ultragraphdb_development:dev
docker cp ultragraphdb:/data/config/bootstrap.secret .
```

A load target loads every platform of its index unless `--platform` names one, and Docker's own store takes one platform
at a time. `//ultragraphdb/ultragraphdb_supervisor_tests:image_bdd` runs the `@image` scenarios against these images.

## Checking an image

`docker buildx imagetools inspect <ref>` reads a pushed index from the registry: expect one entry per platform,
`linux/amd64` and `linux/arm64`, and no `unknown/unknown` entry. For an image built but not pushed, the index is
`<name>_index.json` under `target-bzl/bin/<package>/`, and each manifest's config, with its layers' history, sits in the
output directory of its platform's configuration under `bazel info -c opt output_path`.
