# Copyright (c) 2026 Erick Bourgeois, sceau
# SPDX-License-Identifier: Apache-2.0
#
# Distroless production Dockerfile for the sceau KMS plugin.
#
# This Dockerfile expects a pre-built Linux binary at
# `binaries/<TARGETARCH>/<BINARY>` plus the TPM2 TSS runtime libraries staged
# at `binaries/<TARGETARCH>/rootfs/` — both produced by the Makefile's
# `build-linux-*` targets. We never compile inside the image build.
#
# The binary links dynamically against libtss2 (esys, sys, mu, tctildr, and
# the dlopen'd TCTI modules), which the distroless base does not ship, so the
# staged rootfs is copied over / before the binary lands.
#
# Build with:
#     make docker-image              # ARCH defaults to amd64
#     make docker-image ARCH=arm64   # linux/arm64

# ── Base image ───────────────────────────────────────────────────────────────
# Pinned by digest for supply-chain reproducibility. Do NOT revert to a
# floating tag.
#
# The digest MUST sit on a literal `FROM` line. Dependabot's Docker parser
# reads `FROM` instructions and does not expand `ARG` defaults
# (dependabot/dependabot-core#4597, #10190), so the earlier
# `ARG BASE_IMAGE=<digest>` + `FROM ${BASE_IMAGE}` shape made this base image
# invisible to dependency updates — nothing ever proposed a new digest.
#
# `BASE_IMAGE` still redirects the base for air-gapped / internal-mirror builds
# (docs/src/guides/internal-registry.md). It defaults to the *stage name*, not
# to a registry reference: unset resolves to the pinned digest below, set
# resolves to the caller's mirror. Never put a registry reference in that
# default — that is precisely what hid the image from Dependabot.
#
# When BASE_IMAGE is overridden, BuildKit prunes the unreferenced
# `pinned-base` stage from the build graph, so an air-gapped build still never
# reaches out to gcr.io.
ARG BASE_IMAGE=pinned-base

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:c31ff9abcb1910f3ab25c7957bdaf0bfe12a01eb546e8df2282f1c8f682b606c AS pinned-base

FROM ${BASE_IMAGE}

ARG VERSION
ARG GIT_SHA
ARG TARGETARCH
ARG BINARY=sceau

# Reference recorded in org.opencontainers.image.base.name. Supplied by the
# Makefile, which resolves it to whatever the build actually used: the
# BASE_IMAGE override when set, otherwise the pinned `FROM` read out of this
# file. Never hardcode the upstream registry here — an air-gapped build from an
# internal mirror would then ship a label naming a registry it never contacted.
# `BASE_IMAGE` itself is unusable for this: it holds the stage name by default.
ARG BASE_IMAGE_REF

LABEL org.opencontainers.image.source="https://github.com/firestoned/sceau" \
      org.opencontainers.image.description="sceau — Kubernetes KMS v2 plugin: TPM 2.0 sealed encryption at rest" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.base.name="${BASE_IMAGE_REF}"

# TPM2 TSS runtime libraries (libtss2-esys and friends, including the TCTI
# modules dlopen'd at runtime), staged by `make build-linux-*`.
COPY binaries/${TARGETARCH}/rootfs/ /

# Pre-built binary for the target architecture.
COPY --chmod=755 binaries/${TARGETARCH}/${BINARY} /usr/local/bin/sceau

USER nonroot

ENTRYPOINT ["/usr/local/bin/sceau"]
