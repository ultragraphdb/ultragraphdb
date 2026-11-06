"""Multi-arch images: scratch for statically linked binaries, the glibc base for the server."""

load("@rules_img//img:image.bzl", "image_index", "image_manifest")
load("@rules_img//img:layer.bzl", "image_layer")

# Scratch plus CA certificates: a static binary carries no trust store, and TLS without one fails
# at the first connection. Built from source, its CA bundle pinned by SHA-256 in MODULE.bazel.
SCRATCH_BASE = "//bazel/oci_images/scrach_base:scratch_base"

# glibc, its loader, libgcc and CA certificates from Wolfi, built by rules_apko from a lock file (D7).
GLIBC_BASE = "//bazel/oci_images/glibc_base:glibc_base_index"

# The glibc triples the server image is built for. Must also be in RUST_PLATFORMS.
GNU_PLATFORMS = [
    "@rules_rs//rs/platforms:x86_64-unknown-linux-gnu",
    "@rules_rs//rs/platforms:aarch64-unknown-linux-gnu",
]

# musl, because those are the triples that link statically. Must also be in RUST_PLATFORMS.
MUSL_PLATFORMS = [
    "@rules_rs//rs/platforms:x86_64-unknown-linux-musl",
    "@rules_rs//rs/platforms:aarch64-unknown-linux-musl",
]

def scratch_multiarch_img(name, binary, **kwargs):
    """A multi-arch scratch image containing one binary.

    The binary lands at `/<target name>` and is the entrypoint, so `binary = ":import"` gives an
    image running `/import`. Build with `-c opt`, or the image carries an unstripped debug binary
    five times the size.

    A macro, not a rule: rules cannot instantiate other rules. The intermediates stay queryable as
    `<name>_layer` and `<name>_manifest`.

    Args:
        name: The image index target.
        binary: The binary to package. `image_index` transitions it per platform, so a plain
            `rust_binary` is correct here.
        **kwargs: Forwarded to the index target.
    """
    entrypoint_path = "/" + binary.split(":")[-1].split("/")[-1]

    # The binary alone: a Rust binary reads no runfiles, so its runfiles tree would only be a second copy of it.
    image_layer(
        name = name + "_layer",
        srcs = {entrypoint_path: binary},
        include_runfiles = False,
    )

    # Manual: the base holds Linux images only, so the manifest builds through the index's platform
    # transition, never on its own for the host.
    image_manifest(
        name = name + "_manifest",
        base = SCRATCH_BASE,
        entrypoint = [entrypoint_path],
        layers = [name + "_layer"],
        tags = ["manual"],
    )

    image_index(
        name = name,
        manifests = [name + "_manifest"],
        platforms = MUSL_PLATFORMS,
        **kwargs
    )

def glibc_multiarch_img(name, binary, **kwargs):
    """A multi-arch image of one dynamically linked glibc binary on the glibc base.

    As `scratch_multiarch_img`, with the binary built for the gnu triples and run as the base's
    unprivileged user. Build with `-c opt`.

    Args:
        name: The image index target.
        binary: The binary to package, transitioned per platform by `image_index`.
        **kwargs: Forwarded to the index target.
    """
    entrypoint_path = "/" + binary.split(":")[-1].split("/")[-1]

    # The binary alone: a Rust binary reads no runfiles, so its runfiles tree would only be a second copy of it.
    image_layer(
        name = name + "_layer",
        srcs = {entrypoint_path: binary},
        include_runfiles = False,
    )

    # Manual: the base holds Linux images only, so the manifest builds through the index's platform
    # transition, never on its own for the host.
    image_manifest(
        name = name + "_manifest",
        base = GLIBC_BASE,
        entrypoint = [entrypoint_path],
        layers = [name + "_layer"],
        tags = ["manual"],
    )

    image_index(
        name = name,
        manifests = [name + "_manifest"],
        platforms = GNU_PLATFORMS,
        **kwargs
    )

def sha256_tag(name, image, length = 8, **kwargs):
    """A deterministic tag naming an image by its own content, e.g. `458b6779`.

    The first `length` characters of the SHA-256 of the index document, which is what an OCI index
    digest is. The same image always tags the same.

    Args:
        name: Target name. The tag is written to `<name>.txt`.
        image: The image index to tag.
        length: Hex characters to keep.
        **kwargs: Forwarded to the genrule.
    """

    # sha256sum on Linux, shasum on macOS.
    native.genrule(
        name = name,
        srcs = [image],
        outs = [name + ".txt"],
        cmd = """
set -eu
if command -v sha256sum >/dev/null 2>&1; then
  DIGEST=$$(sha256sum $(location {image}) | cut -d' ' -f1)
else
  DIGEST=$$(shasum -a 256 $(location {image}) | cut -d' ' -f1)
fi
echo "$${{DIGEST}}" | cut -c1-{length} > $@
""".format(image = image, length = length),
        **kwargs
    )
