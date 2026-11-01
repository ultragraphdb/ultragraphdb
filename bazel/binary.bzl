"""Rust binaries built for release."""

load("@crates//:defs.bzl", "aliases", "lint_config")
load("@rules_rs//rs:rust_binary.bzl", "rust_binary")
load("@rules_rs//rs:rust_library.bzl", "rust_library")

# What `cargo build --release` would do, said explicitly.
#
# Bazel's `-c opt` reaches rustc with `--codegen=opt-level=3` and `--codegen=strip=debuginfo`.
# These add what it leaves out: one codegen unit, so inlining crosses the whole crate, and a full
# symbol strip rather than debuginfo alone. Together they take the binary from 17M to 3.2M,
# measured. Verify with:
#
#     bazel aquery 'mnemonic("Rustc", //path:target)' -c opt --output=text
#
# What is NOT here, and why, all measured on aarch64-unknown-linux-musl:
#
#   -Clto              rejected outright: "options `-C embed-bitcode=no` and `-C lto` are
#                      incompatible". rules_rust passes -Cembed-bitcode=no, and overriding it here
#                      does not help -- the dependency rlibs are still built without bitcode, so
#                      cross-crate LTO has nothing to work with. It would need a global setting.
#   -Clink-arg=-flto   the usual workaround for the above, and a no-op for the same reason:
#                      byte-identical output, 3384480 either way.
#   -Clink-arg=-s      no-op alongside -Cstrip=symbols, byte-identical.
#
#   -Cpanic=unwind     preserves drop, which is critical for security code relying on drop.
RELEASE_FLAGS = [
    "-Copt-level=3",
    "-Ccodegen-units=1",
    "-Cstrip=symbols",
    "-Cpanic=unwind",
]

def static_binary(
        name,
        srcs,
        library = None,
        deps = [],
        crate_root = "src/lib.rs",
        main = "src/main.rs",
        release_flags = RELEASE_FLAGS,
        **kwargs):
    """A Rust release binary, and the library it wraps where one is wanted.

    One declaration, so every binary in this repository is built with the same flags. A hand
    written `rust_binary` carries its own copy of the release policy, and a second copy drifts in
    silence: both builds succeed, and only the shipped artifact differs.

    Pass `library` when another crate imports this one. `dgraph_migration` does, because
    `dgraph_import` calls `Mode` and `run_with_client`. The macro then routes `main` to the binary
    and every other source to the library. Leave `library` out for a service whose whole content is
    reached from `main.rs`, and the binary takes every source directly -- which also puts all of it
    in the crate that receives `release_flags`, since those reach the binary.

    The name describes the intent rather than the mechanism: static linking comes from building for
    a musl platform, where it is what the linker does by default:

        bazel build //path:name --platforms=@rules_rs//rs/platforms:x86_64-unknown-linux-musl -c opt

    Measured on the resulting ELF: no `PT_INTERP` segment, zero `DT_NEEDED` entries, no `.symtab`.
    The kernel execs it with no loader and no libc present, which is what a scratch image offers.
    Built for a gnu triple it links dynamically, and the flags still apply.

    The default branch of the flag select is empty on purpose. fastbuild is already `-Copt-level=0`,
    and repeating it would only be a second place to disagree with Bazel about what a debug build
    means.

    Args:
        name: The binary target. This is what goes into an image.
        srcs: Every source, `main` included -- pass the whole glob.
        library: The library target, for a crate that dependents `use`. Name it after the crate.
            Omit it to build the binary alone.
        deps: Dependencies of the binary, and of the library where there is one. The binary
            additionally gets the library.
        crate_root: The library's crate root. Read only when `library` is set.
        main: The binary's crate root, and the one file the library excludes.
        release_flags: rustc flags applied under `-c opt`. Defaults to [RELEASE_FLAGS], which is
            what a shipped binary wants, `-Cpanic=unwind` included so `Drop` still runs when a
            thread panics. Pass `[]` for a target that should take Bazel's `-c opt` settings and
            nothing further.

            REPLACES the default; it does not extend it. Write the complete list you want. Adding
            to [RELEASE_FLAGS] hands rustc the same `-C` option twice -- `-Copt-level=3` and
            `-Copt-level=z` both arrive, one silently wins, and which one is not something the
            BUILD file shows you.
        **kwargs: Forwarded to every target generated. `tags` and `visibility` in practice.

    Generated targets:
        `<name>`: the binary.
        `<library>`: the library, when `library` is set. Doc targets and dependents point at it;
            where it is omitted, they point at `<name>`.
    """
    if library:
        rust_library(
            name = library,
            srcs = [src for src in srcs if src != main],
            aliases = aliases(),
            crate_root = crate_root,
            lint_config = lint_config(),
            deps = deps,
            **kwargs
        )
        binary_srcs = [main]
        binary_deps = [":" + library] + deps
    else:
        binary_srcs = srcs
        binary_deps = deps

    rust_binary(
        name = name,
        srcs = binary_srcs,
        aliases = aliases(),
        crate_root = main,
        lint_config = lint_config(),
        rustc_flags = select({
            "//:release": release_flags,
            "//conditions:default": [],
        }),
        deps = binary_deps,
        **kwargs
    )
