# Copyright (c) 2026 Erick Bourgeois, sceau
# SPDX-License-Identifier: Apache-2.0
#
# ClusterFuzzLite builder image (ADR-0003).
#
# ── Why the default (focal) builder, and not the ubuntu-24-04 variant ────────
#
# The builder must match the RUNNER's glibc. ClusterFuzzLite's actions hardcode
# their container images -- there is no input to select an OS variant -- and
# clusterfuzzlite-run-fuzzers:v1 is Ubuntu 20.04 (glibc 2.31).
#
# This file previously used base-builder-rust:ubuntu-24-04 to get a libtss2-dev
# new enough for tss-esapi-sys. That builds, and then every target dies at
# startup in the runner:
#
#     libc.so.6: version `GLIBC_2.39' not found (required by envelope_decode)
#     libc.so.6: version `GLIBC_2.34' not found (required by lib/libtss2-esys.so.0)
#
# (run 34172370455 -- 100% of fuzz targets "broken"). Binaries linked against
# glibc 2.39 cannot run on 2.31, and no amount of library staging fixes an ABI
# gap in the C runtime itself.
#
# So: build on focal, and solve the real constraint -- focal's libtss2-dev is
# 2.3.2 while tss-esapi-sys 0.6 requires >= 2.4.6 for tss2-{sys,esys,tctildr,mu}
# (its build.rs MINIMUM_VERSION) -- by compiling tpm2-tss from source here.
#
# Verified properties of this image (from its own config blob, not assumed):
#   - org.opencontainers.image.version=20.04  -> glibc 2.31, matches the runner
#   - RUSTUP_TOOLCHAIN=nightly-2025-09-05     -> no toolchain install needed
#   - cargo-fuzz preinstalled (base-builder/install_rust.sh)
#   - libssl-dev already present, so tpm2-tss's OpenSSL dependency is satisfied

FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:433319ea1a6bbea6c1db1915a5d385021908cc62026d9f0efda2270ef6060de3

# Pinned by version + checksum, like every other third-party artefact in this
# repo. 4.1.4 is the current 4.1.x maintenance release; refresh alongside the
# libtss2 version the production Dockerfile stages.
ARG TPM2_TSS_VERSION=4.1.4
ARG TPM2_TSS_SHA256=b18b3496dcf0e2343c3c2f0e355d8171d4a08197a948c00d817aeb6b851927ad
ARG TSS_COMPAT_WARNINGS="-Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-function-pointer-types -Wno-error=deprecated-declarations"

# `sceau` also generates protobuf code at build time and probes libtss2 through
# pkg-config; neither tool ships in the base image.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# --disable-fapi + --disable-policy: both pull in json-c (and FAPI additionally
# libcurl), and tss-esapi needs neither -- it probes only tss2-sys, tss2-esys,
# tss2-tctildr and tss2-mu.
#
# CFLAGS/CXXFLAGS/LDFLAGS are reset for this build: the base image exports
# OSS-Fuzz's instrumentation flags, and letting them apply here would produce a
# sanitizer-instrumented shared library. Leaving tpm2-tss uninstrumented keeps
# the same coverage boundary the distro-packaged libtss2 gave us before this
# change. Instrumenting it would fuzz deeper into the C unmarshal code, which
# is worth doing later -- but as a deliberate change with its own verification,
# not as a side effect of a build-environment fix.
#
# The -Wno-error=* flags are carried over from the base image's own CFLAGS
# rather than dropped with the rest. They are not cosmetic: this image's clang
# is far newer than focal's, and clang 16+ promoted implicit-function-
# declaration and int-conversion from warnings to errors. OSS-Fuzz sets these
# escapes precisely so pre-C99-era C still compiles; resetting CFLAGS without
# them would be a self-inflicted build break.
RUN set -eux; \
    curl -fsSLo /tmp/tpm2-tss.tar.gz \
    "https://github.com/tpm2-software/tpm2-tss/releases/download/${TPM2_TSS_VERSION}/tpm2-tss-${TPM2_TSS_VERSION}.tar.gz"; \
    echo "${TPM2_TSS_SHA256}  /tmp/tpm2-tss.tar.gz" | sha256sum -c -; \
    tar xzf /tmp/tpm2-tss.tar.gz -C /tmp; \
    cd "/tmp/tpm2-tss-${TPM2_TSS_VERSION}"; \
    export CFLAGS="-O2 -g ${TSS_COMPAT_WARNINGS}"; \
    export CXXFLAGS="-O2 -g ${TSS_COMPAT_WARNINGS}"; \
    export LDFLAGS=""; \
    ./configure \
    --prefix=/usr/local \
    --disable-fapi \
    --disable-policy \
    --disable-doxygen-doc \
    --disable-static \
    --disable-dependency-tracking; \
    make -j"$(nproc)"; \
    make install; \
    ldconfig; \
    rm -rf /tmp/tpm2-tss*; \
    pkg-config --modversion tss2-esys tss2-sys tss2-mu tss2-tctildr

ENV PKG_CONFIG_PATH=/usr/local/lib/pkgconfig

COPY . $SRC/sceau
WORKDIR $SRC/sceau
COPY .clusterfuzzlite/build.sh $SRC/build.sh
