#!/bin/bash -eu
# Copyright (c) 2026 Erick Bourgeois, sceau
# SPDX-License-Identifier: Apache-2.0
#
# ClusterFuzzLite build script (ADR-0003): compiles every cargo-fuzz target
# with sanitizers and stages the binaries -- plus the shared libraries they
# need at runtime -- in $OUT.

# Everything this build needs is provisioned in the Dockerfile: pkg-config and
# protobuf-compiler via apt, libtss2 compiled from source into /usr/local
# (focal's packaged 2.3.2 is older than tss-esapi-sys's 2.4.6 minimum), and the
# nightly toolchain + cargo-fuzz shipped by the base image itself.

cd "$SRC/sceau"

TARGETS=(envelope_decode kms_proto_decode)
BUILD_DIR="fuzz/target/x86_64-unknown-linux-gnu/release"

# ── Why the targets ship their own libtss2 ───────────────────────────────────
#
# The targets are EXECUTED in the base-runner image, not in this builder:
# bad_build_check moves everything in $OUT to a temp directory and runs each
# target from there. That image has no libtss2, so a target that merely links
# against it dies at startup with "error while loading shared libraries:
# libtss2-esys.so.0" and is reported as a broken build. That is exactly what
# happened on run 34170567314 -- both targets compiled, then 100% were
# declared broken.
#
# The dependency is real rather than incidental, so it cannot simply be
# dropped: sceau::tpm::envelope_decode calls Public::unmarshall, which is an
# FFI call into Tss2_MU_TPMT_PUBLIC_Unmarshal in libtss2-mu. The fix is the
# standard OSS-Fuzz arrangement -- ship the libraries in $OUT/lib and point
# the loader next to the executable.
#
# Staging is still required now that the Dockerfile builds tpm2-tss itself:
# the libraries land in /usr/local/lib of the *builder*, which the runner
# never sees. What the source build fixes is the other half of the problem --
# those libraries, and the targets linking them, are now built against the
# runner's glibc (2.31) instead of 24.04's 2.39.
#
# Two details this depends on, both verified rather than assumed:
#
#   1. $ORIGIN/lib, not /out/lib. test_all.py moves the *contents* of $OUT
#      wholesale into the temp directory (move_directory_contents), so lib/
#      travels with the binaries and an $ORIGIN-relative path keeps resolving.
#      A hardcoded /out/lib would break there by design -- moving $OUT aside
#      is how OSS-Fuzz catches targets that depend on its absolute path.
#
#   2. --disable-new-dtags, so the linker emits DT_RPATH instead of the modern
#      default DT_RUNPATH. Per ld.so(8), DT_RUNPATH applies *only* to the
#      declaring object's direct DT_NEEDED entries and is not inherited by
#      their children, whereas DT_RPATH applies to the whole dependency tree.
#      With RUNPATH the loader would find libtss2-esys.so.0 next to the binary
#      and then fail on *its* dependency libtss2-sys.so.1 -- the same startup
#      crash, one level deeper.
#
# cargo-fuzz appends $RUSTFLAGS to the flags it generates itself, so setting
# it here composes with the sanitizer flags rather than replacing them.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,\$ORIGIN/lib -C link-arg=-Wl,--disable-new-dtags"

cargo fuzz build -O

mkdir -p "$OUT/lib"
for target in "${TARGETS[@]}"; do
    cp "$BUILD_DIR/$target" "$OUT/"
done

# Ship every shared library the targets resolve, except the core C runtime and
# the dynamic loader: $OUT/lib is searched ahead of the system paths, and a
# libc that disagrees with the runner's loader would break every target
# instead of fixing one. `ldd` reports the full transitive closure, so a
# single pass per target is enough.
for target in "${TARGETS[@]}"; do
    ldd "$OUT/$target" | awk '/=> \//{print $3}' | while read -r so; do
        case "$(basename "$so")" in
            libc.so.* | libm.so.* | libdl.so.* | librt.so.* | libpthread.so.* | \
                libgcc_s.so.* | libstdc++.so.* | ld-linux*)
                continue
                ;;
        esac
        # Explicit existence test rather than `cp -n`, whose exit status on a
        # skipped file is not consistent across coreutils versions -- and this
        # script runs under `set -e`.
        dest="$OUT/lib/$(basename "$so")"
        [ -e "$dest" ] || cp -L "$so" "$dest"
    done
done

echo "== shared libraries staged in \$OUT/lib =="
ls -l "$OUT/lib"

# Verify the arrangement the way the RUNNER will see it. Plain `ldd` here
# would prove nothing: this builder has libtss2 installed system-wide, and
# that difference is the entire bug. So check the ELF directly -- every
# DT_NEEDED entry must be either core runtime (present in any base image) or
# a file we just staged. Failing here gives a one-line reason at build time
# instead of an opaque "100% of fuzz targets seem to be broken" 3 minutes
# later in the runner.
READELF=""
# ${LLVM:-} rather than $LLVM: this script runs under `set -u`, and the
# variable is only defined in some OSS-Fuzz base images.
for candidate in readelf llvm-readelf "${LLVM:-}/bin/llvm-readelf" eu-readelf; do
    if command -v "$candidate" > /dev/null 2>&1; then
        READELF="$candidate"
        break
    fi
done

if [ -z "$READELF" ]; then
    # Skip rather than fail: this check is a faster diagnosis of a failure the
    # runner would catch anyway, so a missing readelf must not become a new
    # way for the build to break.
    echo "WARNING: no readelf found; skipping the DT_NEEDED staging check" >&2
else
    core_libs='^(libc|libm|libdl|librt|libpthread|libgcc_s|libstdc\+\+)\.so|^ld-linux'
    missing=0
    for target in "${TARGETS[@]}"; do
        while read -r needed; do
            [[ "$needed" =~ $core_libs ]] && continue
            if [ ! -e "$OUT/lib/$needed" ]; then
                echo "ERROR: $target needs $needed, which is not staged in \$OUT/lib" >&2
                missing=1
            fi
        done < <("$READELF" -d "$OUT/$target" | awk -F'[][]' '/NEEDED/{print $2}')
    done
    [ "$missing" -eq 0 ] || exit 1
    echo "== all DT_NEEDED entries resolved from \$OUT/lib or core runtime =="
fi
