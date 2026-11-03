# rust-scratch

A scratch image for static Rust binaries: the CA bundle and the unprivileged user `nobody`, nothing else. Multi-arch,
`linux/amd64` and `linux/arm64`, with two layers: `/etc/passwd` and `/etc/group` for `nobody` (65534), and Mozilla's
CA bundle at `/etc/ssl/certs/ca-certificates.crt`. It runs as `nobody`, and images built on it inherit that user.

Bazel builds it from this package with rules_img, and this repository's scratch images build on the target directly.
How it is built, and how to update the CA bundle, is in `bazel/README_BAZEL.md`. This file covers the copy published
to Docker Hub as `hansenmarvin/rust-scratch`.

## Build and push

```bash
bazel build //bazel/oci_images/scrach_base:scratch_base
bazel run //bazel/oci_images/scrach_base:push
```

The push target sends both tags in its `tag_list`, the version tag and `latest`, from the one index, so both name the
same digest. Credentials come from `docker login`.

rules_img writes no attestation manifests into the index, so build tools that read the index directly, rules_img's
`pull` among them, see two image manifests and nothing else.

## Verify the published manifest

```bash
docker buildx imagetools inspect hansenmarvin/rust-scratch:latest
```

Expect exactly two entries, `linux/amd64` and `linux/arm64`, and no `unknown/unknown` entry. An `unknown/unknown` child
is an attestation manifest.

## Sign and attest

cosign attaches signatures and attestations as separate objects in the registry. The image index is untouched and its
digest does not change, so a consumer that pins the digest is unaffected and tooling that reads the index never sees
them.

Both tags point at the same index digest, so one signature covers both. Resolve that digest and sign it, never a tag:
signing `:latest` signs whatever it pointed at that moment.

```bash
DIGEST=$(docker buildx imagetools inspect hansenmarvin/rust-scratch:latest --format '{{.Manifest.Digest}}')
REF=hansenmarvin/rust-scratch@$DIGEST
```

Generate an SBOM and attach both:

```bash
syft "$REF" -o spdx-json > sbom.spdx.json
cosign sign "$REF"
cosign attest --type spdxjson --predicate sbom.spdx.json "$REF"
```

Signing is keyless: cosign mints an ephemeral key, Fulcio issues a short-lived certificate bound to an OIDC identity,
and the result is recorded in the Rekor transparency log. A browser window opens for the OIDC flow. No private key is
stored anywhere.

Docker Hub's support for the OCI 1.1 referrers API is inconsistent. If either command fails while uploading, force the
tag-based fallback, which Docker Hub has always supported:

```bash
cosign sign --registry-referrers-mode=legacy "$REF"
```

### Verify the signature

```bash
cosign verify "$REF" --certificate-identity-regexp '.*' --certificate-oidc-issuer-regexp '.*'
```

It reports that the cosign claims were validated, that the claims' existence in the transparency log was verified
offline, and that the code-signing certificate was verified against trusted certificate authority certificates.

Tighten both regexes to the actual signing identity once it is settled: `.*` accepts any signer and proves only that
somebody signed it.

## Move a tag by pushing, never with `imagetools create`

`docker buildx imagetools create -t ...:latest ...:<version>` rebuilds an index rather than copying it, and re-attaches
any attestation manifests it finds in the registry for the child images. Those survive from earlier builds, so the
rebuilt index can be byte-identical to an old one with `unknown/unknown` children, and `latest` silently keeps pointing
at it. Passing the source by digest does not help, because the problem is the re-attachment, not the resolution.

Push every tag from the build instead, as the push target does, and inspect the tag against the registry afterwards.

## Pull the image

```bash
docker pull hansenmarvin/rust-scratch:latest
```

## Consuming it from another Bazel repository

With rules_img, pin the index digest rather than a per-architecture one, so a single reference resolves for both:

```python
pull(
    name = "rust_scratch",
    digest = "sha256:...",           # the index digest
    registry = "index.docker.io",
    repository = "hansenmarvin/rust-scratch",
    tag = "<version>",
)
```

The digest is what pins it; the tag is recorded for readability. A floating tag such as `latest` would let the trust
store change under a build that never asked for it, and a CA bundle is updated deliberately.
