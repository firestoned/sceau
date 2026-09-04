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

# Pinned by digest for supply-chain reproducibility. Dependabot (docker
# ecosystem) opens a PR with the new digest when upstream publishes a patched
# image. Do NOT revert to a floating tag.
ARG BASE_IMAGE=gcr.io/distroless/cc-debian13:nonroot@sha256:d97bc0a941b8d4be647dc0ee75b264ddbb772f1ac5ba690a4309c00723b23775

FROM ${BASE_IMAGE}

ARG VERSION
ARG GIT_SHA
ARG TARGETARCH
ARG BASE_IMAGE
ARG BINARY=sceau

LABEL org.opencontainers.image.source="https://github.com/firestoned/sceau" \
      org.opencontainers.image.description="sceau — Kubernetes KMS v2 plugin: TPM 2.0 sealed encryption at rest" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.base.name="${BASE_IMAGE}"

# TPM2 TSS runtime libraries (libtss2-esys and friends, including the TCTI
# modules dlopen'd at runtime), staged by `make build-linux-*`.
COPY binaries/${TARGETARCH}/rootfs/ /

# Pre-built binary for the target architecture.
COPY --chmod=755 binaries/${TARGETARCH}/${BINARY} /usr/local/bin/sceau

USER nonroot

ENTRYPOINT ["/usr/local/bin/sceau"]
