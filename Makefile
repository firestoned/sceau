# Copyright (c) 2026 Erick Bourgeois, sceau
# SPDX-License-Identifier: Apache-2.0
#
# sceau — Kubernetes KMS v2 plugin: TPM 2.0 sealed encryption at rest.
#
# This Makefile is the single source of workflow truth for both local
# development and CI. Conventions follow the banlieue project pattern:
#
#   - All workflow logic lives here, not in workflow YAML.
#   - Container images are built from a pre-built Linux binary (compiled in a
#     rust:1-bookworm build container with libtss2-dev) — never `cargo build`
#     inside the image build.
#   - One distroless Dockerfile; the TSS runtime libraries the binary links
#     against are staged into the image rootfs by `build-linux-*`.
#
# Local dev loop:
#
#   make build test lint            # build, test, fmt+clippy
#   make docker-image ARCH=amd64    # distroless image from prebuilt binary

.DEFAULT_GOAL := help

# ----- Variables ------------------------------------------------------------

BINARY  ?= sceau

# Image configuration
REGISTRY     ?= ghcr.io
ORG          ?= firestoned
IMAGE_TAG    ?= latest-dev
IMAGE_REF    ?= $(REGISTRY)/$(ORG)/$(BINARY):$(IMAGE_TAG)

# Target architecture for docker-image / build-linux-* (amd64 | arm64).
ARCH ?= amd64

# Base image (pinned by digest in the Dockerfile; this is the default ARG)
BASE_IMAGE ?= gcr.io/distroless/cc-debian13:nonroot

# Build container used to produce the Linux binary (has cargo + apt for
# libtss2-dev). The image build itself never compiles.
RUST_BUILD_IMAGE ?= rust:1-bookworm

# Version information
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
GIT_SHA ?= $(shell git rev-parse --verify -q HEAD 2>/dev/null || echo "unknown")

# Container tool (docker or podman)
CONTAINER_TOOL ?= docker

# CALM (FINOS Common Architecture Language Model) configuration
CALM_CLI_VERSION  ?= 1.37.0
CALM_ARCH          := docs/architecture/calm/architecture.json
CALM_TEMPLATES     := docs/architecture/calm/templates/mermaid
CALM_DIAGRAMS_OUT  := docs/architecture

# ----- Help -----------------------------------------------------------------

help: ## Show this help
	@echo 'Usage: make [target] [VAR=value ...]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-28s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@echo ''
	@echo 'Common variables:'
	@echo '  ARCH=<amd64|arm64>      (default: $(ARCH))'
	@echo '  IMAGE_TAG=<tag>         (default: $(IMAGE_TAG))'
	@echo '  REGISTRY=<registry>     (default: $(REGISTRY))'

.PHONY: help build build-debug build-linux-amd64 build-linux-arm64 \
        test lint format audit deny sbom clean \
        calm-validate calm-diagrams docker-image docker-push

# ----- Development ----------------------------------------------------------

build: ## Build the sceau binary (release)
	cargo build --release

build-debug: ## Build the sceau binary (debug)
	cargo build

test: ## Run all tests
	cargo test --all-features

lint: ## Check formatting and run clippy with -D warnings
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

format: ## Format all code
	cargo fmt --all

clean: ## Clean build artefacts and staged binaries
	cargo clean
	rm -rf binaries/

# ----- Security / supply chain ----------------------------------------------

audit: ## Run cargo-audit against Cargo.lock
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit --locked
	cargo audit

deny: ## Run cargo-deny (licenses, advisories, sources)
	@command -v cargo-deny >/dev/null 2>&1 || cargo install cargo-deny --locked
	cargo deny check

sbom: ## Generate a CycloneDX SBOM (sceau.cdx.json)
	@command -v cargo-cyclonedx >/dev/null 2>&1 || cargo install cargo-cyclonedx --locked
	@cargo cyclonedx --format json
	@echo "✓ CycloneDX SBOM generated"

# ----- CALM (architecture-as-code, FINOS) -----------------------------------

calm-validate: ## Validate the CALM architecture against the meta-schema
	@command -v npx >/dev/null 2>&1 || { echo "Error: npx not found. Install Node.js from https://nodejs.org"; exit 1; }
	@npx --yes @finos/calm-cli@$(CALM_CLI_VERSION) validate \
	  -a $(CALM_ARCH) \
	  -f pretty

calm-diagrams: ## Render CALM Mermaid diagrams into $(CALM_DIAGRAMS_OUT)
	@command -v npx >/dev/null 2>&1 || { echo "Error: npx not found. Install Node.js from https://nodejs.org"; exit 1; }
	@echo "Rendering CALM diagrams via @finos/calm-cli@$(CALM_CLI_VERSION)..."
	@mkdir -p $(CALM_DIAGRAMS_OUT)
	@rm -f $(CALM_DIAGRAMS_OUT)/system.md $(CALM_DIAGRAMS_OUT)/flows.md $(CALM_DIAGRAMS_OUT)/*.hbs
	@npx --yes @finos/calm-cli@$(CALM_CLI_VERSION) template \
	  -a $(CALM_ARCH) \
	  -d $(CALM_TEMPLATES) \
	  -o $(CALM_DIAGRAMS_OUT)
	@for f in $(CALM_DIAGRAMS_OUT)/*.hbs; do \
	  [ -e "$$f" ] || continue; \
	  mv "$$f" "$${f%.hbs}"; \
	done
	@echo "✓ CALM diagrams written to $(CALM_DIAGRAMS_OUT)/"

# ----- Linux binaries + container image --------------------------------------
#
# The binary links against the TPM2 TSS shared libraries, which the distroless
# base image does not ship. `build-linux-*` therefore stages both the binary
# (binaries/<arch>/sceau) and the TSS runtime libraries — including the TCTI
# modules that libtss2-tctildr dlopen()s — into binaries/<arch>/rootfs/, which
# the Dockerfile copies over /.

define BUILD_LINUX
	$(CONTAINER_TOOL) run --rm --platform linux/$(1) \
	  -v $(CURDIR):/src -w /src $(RUST_BUILD_IMAGE) bash -c '\
	  set -euo pipefail; \
	  apt-get update -qq && apt-get install -y -qq libtss2-dev protobuf-compiler >/dev/null; \
	  cargo build --release; \
	  triplet=$$(gcc -dumpmachine); \
	  mkdir -p binaries/$(1)/rootfs/usr/lib/$$triplet; \
	  cp target/release/$(BINARY) binaries/$(1)/$(BINARY); \
	  cp -L /usr/lib/$$triplet/libtss2*.so.* binaries/$(1)/rootfs/usr/lib/$$triplet/; \
	  echo staged:; ls binaries/$(1) binaries/$(1)/rootfs/usr/lib/$$triplet'
endef

build-linux-amd64: ## Build Linux amd64 binary + TSS libs, staged under binaries/amd64/
	$(call BUILD_LINUX,amd64)

build-linux-arm64: ## Build Linux arm64 binary + TSS libs, staged under binaries/arm64/
	$(call BUILD_LINUX,arm64)

docker-image: build-linux-$(ARCH) ## Build the distroless image $(IMAGE_REF) from the pre-built binary
	$(CONTAINER_TOOL) buildx build --load --platform=linux/$(ARCH) \
	  -t $(IMAGE_REF) \
	  --build-arg BINARY=$(BINARY) \
	  --build-arg VERSION="$(VERSION)" \
	  --build-arg GIT_SHA="$(GIT_SHA)" \
	  --build-arg BASE_IMAGE="$(BASE_IMAGE)" \
	  -f Dockerfile .

docker-push: ## Push $(IMAGE_REF)
	$(CONTAINER_TOOL) push $(IMAGE_REF)
