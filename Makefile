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
#   make docs-serve                 # live-reload docs at http://127.0.0.1:8000

.DEFAULT_GOAL := help

# ----- Variables ------------------------------------------------------------

BINARY  ?= sceau

# Image configuration. REGISTRY is the registry host (it may include a
# namespace path); ORG is appended when non-empty; BINARY is the repo name.
# IMAGE is an alias for IMAGE_TAG (e.g. IMAGE=v0.1.0).
REGISTRY     ?= ghcr.io
ORG          ?= firestoned
IMAGE_TAG    ?= $(if $(IMAGE),$(IMAGE),latest-dev)
IMAGE_REF    ?= $(REGISTRY)$(if $(strip $(ORG)),/$(ORG),)/$(BINARY):$(IMAGE_TAG)

# Push the image after building (PUSH=true) instead of loading it locally.
PUSH ?= false

# Target architecture for docker-image / build-linux-* (amd64 | arm64).
ARCH ?= amd64

# Platform list for `docker-image-prestaged` (comma-separated, buildx syntax).
# Defaults to just $(ARCH); CI passes both to produce a real multi-arch
# manifest from the two pre-staged binary trees.
PLATFORMS ?= linux/$(ARCH)

# buildx writes the pushed manifest digest here. Signing, attesting and
# scanning all key off that digest rather than the tag: a tag is mutable, so
# a signature or scan bound to one says nothing durable (ADR-0006).
DOCKER_METADATA_FILE ?= docker-build-metadata.json

# Base image override, for air-gapped / internal-mirror builds only
# (docs/src/guides/internal-registry.md). EMPTY by default so the Dockerfile's
# own digest-pinned `FROM` wins.
#
# This used to default to the floating tag `gcr.io/distroless/cc-debian13:nonroot`
# and was passed unconditionally as --build-arg, which silently overrode the
# Dockerfile's digest pin on every single build — the image was reproducible in
# the file and not reproducible in practice. Leave it empty unless you are
# genuinely redirecting to a mirror.
BASE_IMAGE ?=

# Only pass the build-arg when an override was actually requested: passing it
# empty would override the Dockerfile's `ARG BASE_IMAGE=pinned-base` with the
# empty string and break `FROM ${BASE_IMAGE}`.
BASE_IMAGE_BUILD_ARG = $(if $(strip $(BASE_IMAGE)),--build-arg BASE_IMAGE="$(BASE_IMAGE)",) --build-arg BASE_IMAGE_REF="$(BASE_IMAGE_REF)"

# What the build actually used, for org.opencontainers.image.base.name: the
# override when set, otherwise the pinned `FROM` read out of the Dockerfile. So
# an air-gapped build labels itself with the mirror it really pulled from, not
# with an upstream registry it never contacted.
BASE_IMAGE_REF = $(if $(strip $(BASE_IMAGE)),$(BASE_IMAGE),$(shell awk '$$1 == "FROM" { print $$2; exit }' Dockerfile))

# Build container used to produce the Linux binary (has cargo + apt for
# libtss2-dev). The image build itself never compiles.
RUST_BUILD_IMAGE ?= rust:1-bookworm

# Optional shell command run verbatim inside RUST_BUILD_IMAGE before
# `apt-get update`, for networks that block deb.debian.org directly. Networks
# with a private apt mirror commonly also need a custom repo layout, a
# separate security repo, a corporate CA bundle, and/or proxy overrides — no
# single templated substitution covers all of that, so this is a raw hook.
# Empty means use the image's built-in sources unmodified. Example:
#   APT_SETUP_CMD='echo "deb https://mirror.example.com/debian trixie main" > /etc/apt/sources.list'
APT_SETUP_CMD ?=
export APT_SETUP_CMD

# Optional host path to a CA certificate/bundle file. When set, it is
# bind-mounted into RUST_BUILD_IMAGE and trusted (`update-ca-certificates`)
# before apt/cargo touch the network — needed on networks where a TLS-
# intercepting proxy or an internal-only registry presents a certificate the
# public base image doesn't already trust (symptom: apt "does not have a
# Release file" / cargo "SSL routines::unexpected eof" against an otherwise
# correct mirror URL). Empty means use the image's built-in trust store only.
CA_BUNDLE ?=
export CA_BUNDLE

# Optional Cargo sparse-index mirror (e.g. an Artifactory Cargo remote repo)
# for build-linux-*, for networks that block crates.io directly. Full sparse
# registry URL, e.g.:
#   sparse+https://artifactory.example.com/artifactory/api/cargo/rust-remote/index/
# Empty means use crates.io directly.
CARGO_REGISTRY_MIRROR ?=
export CARGO_REGISTRY_MIRROR

# Version information
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
GIT_SHA ?= $(shell git rev-parse --verify -q HEAD 2>/dev/null || echo "unknown")

# Container tool (docker or podman)
CONTAINER_TOOL ?= docker

# `crane` (go-containerregistry) — used by docker-image to explicitly
# delete an existing tag's manifest before re-pushing it. Reuses whatever
# credential helper `docker login` already configured.
CRANE_TOOL ?= crane

# ----- Supply chain: VEX / Grype / SLSA (ADR-0006) --------------------------

# openvex/vexctl — merges .vex/*.json plus the auto-derived documents into
# the single OpenVEX document CI attests and Grype consumes.
VEXCTL_VERSION ?= 0.4.1

# Grype MUST stay >= 0.118.0. Older releases accept `--vex` and then emit the
# suppressed findings anyway (verified in banlieue against 0.87.0), which
# makes a VEX pipeline look like it works while suppressing nothing. See
# ADR-0006. Do NOT downgrade.
GRYPE_VERSION ?= 0.118.0

# Product identifier stamped into every emitted/curated OpenVEX statement.
PRODUCT_PURL ?= pkg:oci/$(BINARY)

# Inputs for the local `vex-auto-presence` / `grype-*` targets. In CI these
# are passed explicitly; locally they default to whatever the standard
# targets leave behind.
GRYPE_JSON     ?= grype.json
GRYPE_SARIF    ?= grype.sarif
VEX_DOCUMENT   ?= vex.openvex.json
SBOM_FILES     ?= $(wildcard target/release/*.cdx.json *.cdx.json docker-sbom-*.json)

# Image reference the grype-* targets scan. Defaults to the locally built
# image; CI overrides it with a digest reference (never a tag — a tag is
# mutable, and a scan result pinned to one is not evidence about anything).
SCAN_IMAGE_REF ?= $(IMAGE_REF)

# Identity stamped on the assembled OpenVEX document, and where
# `vex-assemble-all` looks for the auto-derived inputs. CI overrides VEX_ID
# with the commit/release URL so a document can be traced to the run that
# produced it.
VEX_ID       ?= https://$(BINARY)/local/assemble
AUTO_VEX_ID  ?= https://$(BINARY)/local/auto-presence
VEX_AUTHOR   ?= $(shell git config user.email 2>/dev/null || echo local)
AUTO_VEX_DIR ?= auto

# CALM (FINOS Common Architecture Language Model) configuration
CALM_CLI_VERSION  ?= 1.37.0
CALM_ARCH          := docs/architecture/calm/architecture.json
CALM_TEMPLATES     := docs/architecture/calm/templates/mermaid
CALM_DIAGRAMS_OUT  := docs/src/architecture

# ----- Help -----------------------------------------------------------------

help: ## Show this help
	@echo 'Usage: make [target] [VAR=value ...]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-28s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@echo ''
	@echo 'Common variables:'
	@echo '  ARCH=<amd64|arm64>      (default: $(ARCH))'
	@echo '  IMAGE=<tag>             image tag, alias of IMAGE_TAG (default: $(IMAGE_TAG))'
	@echo '  REGISTRY=<registry>     (default: $(REGISTRY))'
	@echo '  ORG=<org>               appended to REGISTRY when non-empty (default: $(ORG))'
	@echo '  BASE_IMAGE=<image>      override the runtime base for air-gapped/mirrored builds (default: the Dockerfile FROM pin)'
	@echo '  RUST_BUILD_IMAGE=<img>  build container for build-linux-* fallback (default: $(RUST_BUILD_IMAGE))'
	@echo '  APT_SETUP_CMD=<cmd>     shell command run before apt-get update in the build-linux-* container fallback (default: unset)'
	@echo '  CA_BUNDLE=<path>        host CA cert/bundle trusted inside the build-linux-* container fallback (default: unset)'
	@echo '  CARGO_REGISTRY_MIRROR=<sparse-url>  cargo index mirror for build-linux-* (default: unset)'
	@echo '  PUSH=<true|false>       push instead of --load (default: $(PUSH))'
	@echo '  HOSTS=<user@host ...>   space-separated deploy-test targets (required for deploy-test; never hardcode a real host)'
	@echo '  SSH_KEY=<path>          identity file for deploy-test (default: ssh-agent/config)'
	@echo '  IMAGE_REF=<ref>         image to pull+extract for deploy-test on macOS (required there; unused on Linux)'
	@echo '  SOURCE_MODE=<local|image>  override deploy-test'"'"'s uname-based auto-detection'

.PHONY: help build build-debug build-linux-amd64 build-linux-arm64 \
        test test-tpm lint format audit deny sbom clean \
        vexctl-install grype-install grype-triage grype-scan \
        vex-validate vex-assemble vex-auto-presence vex-assemble-all \
        calm-validate calm-diagrams docker-image docker-image-prestaged \
        docker-digest docker-push deploy-test \
        docs docs-serve docs-clean docs-deploy

# ----- Development ----------------------------------------------------------

build: ## Build the sceau binary (release)
	cargo build --release

build-debug: ## Build the sceau binary (debug)
	cargo build

test: ## Run all tests (workspace: sceau + sceau-vex)
	cargo test --workspace --all-features

# SCEAU_TEST_TCTI must point at a real TPM or a running swtpm, e.g.
#   swtpm socket --tpm2 --tpmstate dir=/tmp/sceau-swtpm \
#     --flags not-need-init,startup-clear \
#     --ctrl type=tcp,port=2322 --server type=tcp,port=2321 &
#   make test-tpm SCEAU_TEST_TCTI="swtpm:host=127.0.0.1,port=2321"
#
# --test-threads=1 is required, not cosmetic: a TPM is a single shared
# resource and these tests persist a fleet key at the same well-known handle,
# so in parallel they race and the loser gets TPM_RC_NV_DEFINED.
test-tpm: ## Run the #[ignore]d TPM integration tests (needs SCEAU_TEST_TCTI)
	@if [ -z "$(SCEAU_TEST_TCTI)" ]; then \
		echo "ERROR: set SCEAU_TEST_TCTI to a TCTI conf string for a real TPM or swtpm," >&2; \
		echo "       e.g. make test-tpm SCEAU_TEST_TCTI=\"swtpm:host=127.0.0.1,port=2321\"" >&2; \
		exit 1; \
	fi
	SCEAU_TEST_TCTI="$(SCEAU_TEST_TCTI)" \
	  cargo test --test fleet_duplication -- --ignored --nocapture --test-threads=1

lint: ## Check formatting and run clippy with -D warnings
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings

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

# cargo-cyclonedx emits one SBOM per workspace member and has no
# package filter. crates/sceau-vex is CI-only tooling that is never
# linked into the released binary, so its SBOM is discarded rather than
# shipped — an SBOM that lists dependencies the artifact does not
# contain is worse than no SBOM. See ADR-0006.
sbom: ## Generate a CycloneDX SBOM for the shipped binary (sceau.cdx.json)
	@command -v cargo-cyclonedx >/dev/null 2>&1 || cargo install cargo-cyclonedx --locked
	@cargo cyclonedx --format json
	@rm -f crates/*/*.cdx.json
	@echo "✓ CycloneDX SBOM generated (sceau.cdx.json)"

# ----- Supply chain: VEX / Grype (ADR-0006) ---------------------------------

vexctl-install: ## Install openvex/vexctl ($(VEXCTL_VERSION)) if not already present
	@if command -v vexctl >/dev/null 2>&1; then echo "vexctl already installed"; exit 0; fi; \
	if [ "$$(uname -s)" = "Darwin" ]; then \
		brew install vexctl; \
	else \
		arch=$$(uname -m); case "$$arch" in x86_64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; esac; \
		url="https://github.com/openvex/vexctl/releases/download/v$(VEXCTL_VERSION)/vexctl-linux-$$arch"; \
		echo "Downloading $$url"; \
		curl -fsSLo /tmp/vexctl "$$url"; \
		sudo install -m 0755 /tmp/vexctl /usr/local/bin/vexctl; \
		rm -f /tmp/vexctl; \
	fi; \
	vexctl version

grype-install: ## Install anchore/grype ($(GRYPE_VERSION)) if not already present at that version
	@if command -v grype >/dev/null 2>&1 && grype version 2>/dev/null | grep -q "$(GRYPE_VERSION)"; then \
		echo "grype $(GRYPE_VERSION) already installed"; exit 0; \
	fi; \
	os=$$(uname -s | tr '[:upper:]' '[:lower:]'); \
	arch=$$(uname -m); case "$$arch" in x86_64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; esac; \
	url="https://github.com/anchore/grype/releases/download/v$(GRYPE_VERSION)/grype_$(GRYPE_VERSION)_$${os}_$${arch}.tar.gz"; \
	echo "Downloading $$url"; \
	tmp=$$(mktemp -d); \
	curl -fsSLo "$$tmp/grype.tgz" "$$url"; \
	tar -xzf "$$tmp/grype.tgz" -C "$$tmp" grype; \
	sudo install -m 0755 "$$tmp/grype" /usr/local/bin/grype; \
	rm -rf "$$tmp"; \
	grype version

grype-triage: grype-install ## Raw image scan with NO VEX -> $(GRYPE_JSON) (input for auto-vex-presence)
	@echo "==> Raw (no-VEX) scan of $(SCAN_IMAGE_REF) -> $(GRYPE_JSON)"
	@grype "$(SCAN_IMAGE_REF)" --output json --file "$(GRYPE_JSON)"
	@echo "✓ wrote $(GRYPE_JSON)"

grype-scan: grype-install ## Scan $(SCAN_IMAGE_REF) with VEX suppression -> $(GRYPE_SARIF)
	@echo "==> Scanning $(SCAN_IMAGE_REF) (VEX: $(VEX_DOCUMENT)) -> $(GRYPE_SARIF)"
	@if [ -f "$(VEX_DOCUMENT)" ]; then \
		grype "$(SCAN_IMAGE_REF)" --vex "$(VEX_DOCUMENT)" --output sarif --file "$(GRYPE_SARIF)"; \
	else \
		echo "note: $(VEX_DOCUMENT) not found — scanning without VEX suppression"; \
		grype "$(SCAN_IMAGE_REF)" --output sarif --file "$(GRYPE_SARIF)"; \
	fi
	@echo "✓ wrote $(GRYPE_SARIF)"

vex-validate: vexctl-install ## Validate that every .vex/*.json parses and merges
	@shopt -s nullglob 2>/dev/null || true; \
	set -- .vex/*.json; \
	if [ "$$1" = ".vex/*.json" ] || [ $$# -eq 0 ]; then \
		echo "✓ no curated .vex/*.json statements to validate"; exit 0; \
	fi; \
	vexctl merge --id "https://$(BINARY)/local/validate" --author "local" "$$@" > /dev/null; \
	echo "✓ all $$# curated .vex/*.json document(s) parsed and merged"

vex-assemble: vexctl-install ## Merge .vex/*.json into one OpenVEX document on stdout
	@vexctl merge \
		--id "https://$(BINARY)/local/assemble" \
		--author "$$(git config user.email 2>/dev/null || echo local)" \
		.vex/*.json

# Curated statements may legitimately be zero; auto-derived documents may
# legitimately contain zero statements. Both are normal, so an empty
# result is a valid OpenVEX document rather than a failure — CI must be
# able to run this on a repo with nothing triaged yet (ADR-0006).
vex-assemble-all: vexctl-install ## Merge curated .vex/*.json + $(AUTO_VEX_DIR)/vex.auto-*.json into $(VEX_DOCUMENT)
	@set -e; \
	inputs=""; \
	for f in .vex/*.json $(AUTO_VEX_DIR)/vex.auto-*.json; do \
		[ -f "$$f" ] || continue; \
		inputs="$$inputs $$f"; \
	done; \
	if [ -z "$$inputs" ]; then \
		echo "note: no VEX inputs found — writing an empty OpenVEX document"; \
		printf '{"@context":"https://openvex.dev/ns/v0.2.0","@id":"%s","author":"%s","timestamp":"%s","version":1,"statements":[]}\n' \
			"$(VEX_ID)" "$(VEX_AUTHOR)" "$$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$(VEX_DOCUMENT)"; \
	else \
		echo "==> Merging:$$inputs"; \
		vexctl merge --id "$(VEX_ID)" --author "$(VEX_AUTHOR)" $$inputs > "$(VEX_DOCUMENT)"; \
	fi; \
	echo "✓ wrote $(VEX_DOCUMENT) ($$(grep -o '\"vulnerability\"' "$(VEX_DOCUMENT)" | wc -l | tr -d ' ') statement(s))"

vex-auto-presence: ## Run auto-vex-presence locally ($(GRYPE_JSON) + $(SBOM_FILES) required)
	@if [ ! -f "$(GRYPE_JSON)" ]; then echo "ERROR: $(GRYPE_JSON) not found (run 'make grype-triage SCAN_IMAGE_REF=...')"; exit 1; fi
	@if [ -z "$(SBOM_FILES)" ]; then echo "ERROR: no SBOMs found (target/release/*.cdx.json, *.cdx.json or docker-sbom-*.json)"; exit 1; fi
	@cargo run --quiet -p sceau-vex --bin auto-vex-presence -- \
		--grype-json "$(GRYPE_JSON)" \
		$(foreach s,$(SBOM_FILES),--sbom "$(s)") \
		--vex-dir .vex \
		--product-purl "$(PRODUCT_PURL)" \
		--id "$(AUTO_VEX_ID)" \
		--author auto-vex-presence \
		--output vex.auto-presence.json
	@echo "✓ wrote vex.auto-presence.json"


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

# ----- Documentation (MkDocs Material) --------------------------------------

docs: calm-diagrams ## Build the MkDocs site into docs/site/ (regenerates the CALM diagrams first)
	@command -v poetry >/dev/null 2>&1 || { echo "Error: Poetry not found. Install: curl -sSL https://install.python-poetry.org | python3 -"; exit 1; }
	@echo "Ensuring documentation dependencies are installed..."
	@cd docs && poetry install --no-interaction --quiet
	@echo "Building MkDocs site (strict)..."
	@cd docs && poetry run mkdocs build --strict
	@echo "✓ Documentation built at docs/site/index.html"

docs-serve: ## Serve docs locally with live reload at http://127.0.0.1:8000
	@command -v poetry >/dev/null 2>&1 || { echo "Error: Poetry not found. Install: curl -sSL https://install.python-poetry.org | python3 -"; exit 1; }
	@cd docs && poetry install --no-interaction --quiet
	@echo "Starting MkDocs server at http://127.0.0.1:8000 (live reload)..."
	@cd docs && poetry run mkdocs serve --livereload

docs-clean: ## Remove docs build artefacts, generated diagrams, and venv
	@rm -rf docs/site/ docs/.venv/ docs/poetry.lock
	@rm -f $(CALM_DIAGRAMS_OUT)/system.md $(CALM_DIAGRAMS_OUT)/flows.md
	@echo "✓ Documentation artefacts cleaned"

docs-deploy: docs ## Build and deploy docs to GitHub Pages
	@cd docs && poetry run mkdocs gh-deploy --force
	@echo "✓ Documentation deployed to GitHub Pages"

# ----- Linux binaries + container image --------------------------------------
#
# The binary links against the TPM2 TSS shared libraries, which the distroless
# base image does not ship. `build-linux-*` therefore stages both the binary
# (binaries/<arch>/sceau) and the TSS runtime libraries — including the TCTI
# modules that libtss2-tctildr dlopen()s — into binaries/<arch>/rootfs/, which
# the Dockerfile copies over /.
#
# Unlike a pure-Rust binary, sceau links against libtss2 (tss-esapi), a system
# C library — `cargo build --target <triple>` needs libtss2-dev headers/.so
# for that *target* triple, not just a cross-linker. There is no macOS/Homebrew
# package for cross-compiled libtss2, so the host-toolchain path only succeeds
# when running natively on Linux (CI runners) or when a target sysroot with
# libtss2-dev has been set up by hand. Everywhere else (e.g. macOS dev), this
# falls back to a container that has libtss2-dev installed for the right arch
# — RUST_BUILD_IMAGE stays a variable so it can point at a private registry
# mirror instead of Docker Hub. See ADR-0002 addendum.

build-linux-amd64: ## Build Linux amd64 binary + TSS libs, staged under binaries/amd64/
	@$(MAKE) --no-print-directory _build-linux ARCH_NAME=amd64 TRIPLE=x86_64-unknown-linux-gnu LINKER=x86_64-linux-gnu-gcc

build-linux-arm64: ## Build Linux arm64 binary + TSS libs, staged under binaries/arm64/
	@$(MAKE) --no-print-directory _build-linux ARCH_NAME=arm64 TRIPLE=aarch64-unknown-linux-gnu LINKER=aarch64-linux-gnu-gcc

# Internal: shared cross-compile + stage body. Prefers a host-installed gcc
# cross-toolchain (or running natively on a matching Linux host) over a
# containerized build, and only falls back to the container when libtss2-dev
# isn't linkable for the target triple.
.PHONY: _build-linux
_build-linux:
	@mkdir -p binaries/$(ARCH_NAME)
	@if [ -n "$$CARGO_REGISTRY_MIRROR" ]; then \
		export CARGO_HTTP_MULTIPLEXING=false; \
		set -- --config "source.crates-io.replace-with=\"mirror\"" \
		       --config "source.mirror.registry=\"$$CARGO_REGISTRY_MIRROR\"" \
		       --config "http.ssl-version=\"tlsv1.2\""; \
	else \
		set --; \
	fi; \
	if pkg-config --exists tss2-esys 2>/dev/null && \
	    { command -v $(LINKER) >/dev/null 2>&1 || [ "$$(uname -s)-$$(uname -m)" = "Linux-$${TRIPLE%%-*}" ]; }; then \
		echo "Building natively / via host gcc cross-toolchain for $(TRIPLE)..."; \
		rustup target add $(TRIPLE) >/dev/null 2>&1 || true; \
		triplet=$(TRIPLE); \
		if command -v $(LINKER) >/dev/null 2>&1 && [ "$$(uname -m)" != "$${triplet%%-*}" ]; then \
			TRIPLE_ENV=$$(echo $(TRIPLE) | tr 'a-z-' 'A-Z_'); \
			TRIPLE_US=$$(echo $(TRIPLE) | tr '-' '_'); \
			env CARGO_TARGET_$${TRIPLE_ENV}_LINKER=$(LINKER) \
				CC_$${TRIPLE_US}=$(LINKER) \
				AR_$${TRIPLE_US}=$(LINKER:-gcc=-ar) \
				cargo "$$@" build --release --target $(TRIPLE); \
		else \
			cargo "$$@" build --release --target $(TRIPLE); \
		fi; \
		mkdir -p binaries/$(ARCH_NAME)/rootfs/usr/lib/$(TRIPLE); \
		cp target/$(TRIPLE)/release/$(BINARY) binaries/$(ARCH_NAME)/$(BINARY); \
		cp -L $$(pkg-config --variable=libdir tss2-esys)/libtss2*.so.* binaries/$(ARCH_NAME)/rootfs/usr/lib/$(TRIPLE)/; \
	else \
		echo "No linkable libtss2-dev for $(TRIPLE) on this host; building in $(RUST_BUILD_IMAGE)..."; \
		rcfile=$$(mktemp); \
		{ $(CONTAINER_TOOL) run --rm --platform linux/$(ARCH_NAME) \
		  -e APT_SETUP_CMD \
		  -e CARGO_REGISTRY_MIRROR \
		  $(if $(CA_BUNDLE),-v $(CA_BUNDLE):/usr/local/share/ca-certificates/build-ca-bundle.crt:ro,) \
		  -v $(CURDIR):/src -w /src \
		  -v sceau-cargo-registry:/usr/local/cargo/registry \
		  -v sceau-cargo-git:/usr/local/cargo/git \
		  -v sceau-target-$(ARCH_NAME):/build-target \
		  -e CARGO_TARGET_DIR=/build-target \
		  $(RUST_BUILD_IMAGE) bash -c '\
		  set -euo pipefail; \
		  export DEBIAN_FRONTEND=noninteractive; \
		  if [ -f /usr/local/share/ca-certificates/build-ca-bundle.crt ]; then \
		    update-ca-certificates >/dev/null; \
		    export CARGO_HTTP_CAINFO=/usr/local/share/ca-certificates/build-ca-bundle.crt; \
		  fi; \
		  if [ -n "$$APT_SETUP_CMD" ]; then eval "$$APT_SETUP_CMD"; fi; \
		  if [ -n "$$CARGO_REGISTRY_MIRROR" ]; then \
		    export CARGO_HTTP_MULTIPLEXING=false; \
		    set -- --config "source.crates-io.replace-with=\"mirror\"" \
		           --config "source.mirror.registry=\"$$CARGO_REGISTRY_MIRROR\"" \
		           --config "http.ssl-version=\"tlsv1.2\""; \
		  else \
		    set --; \
		  fi; \
		  apt-get update -qq && apt-get install -y -qq libtss2-dev protobuf-compiler binutils >/dev/null; \
		  export RUSTFLAGS="$${RUSTFLAGS:-} -C link-arg=-fuse-ld=bfd"; \
		  cargo "$$@" build --release; \
		  triplet=$$(gcc -dumpmachine); \
		  mkdir -p binaries/$(ARCH_NAME)/rootfs/usr/lib/$$triplet; \
		  cp "$$CARGO_TARGET_DIR"/release/$(BINARY) binaries/$(ARCH_NAME)/$(BINARY); \
		  cp -L /usr/lib/$$triplet/libtss2*.so.* binaries/$(ARCH_NAME)/rootfs/usr/lib/$$triplet/' 2>&1; echo $$? > "$$rcfile"; } \
		  | grep -v '^<jemalloc>:'; \
		rc=$$(cat "$$rcfile"); rm -f "$$rcfile"; \
		[ "$$rc" -eq 0 ] || exit "$$rc"; \
	fi
	@echo "✓ binaries/$(ARCH_NAME)/$(BINARY) staged"

docker-image: build-linux-$(ARCH) ## Build the distroless image $(IMAGE_REF) from the pre-built binary (PUSH=true to push)
	@echo "==> Building $(IMAGE_REF) for linux/$(ARCH) (PUSH=$(PUSH))"
	@echo "==> BASE_IMAGE=$(if $(BASE_IMAGE),$(BASE_IMAGE) (override),<Dockerfile pinned-base>)"
	@echo "==> VERSION=$(VERSION) GIT_SHA=$(GIT_SHA)"
ifeq ($(filter true,$(PUSH)),true)
	@echo "==> Deleting existing $(IMAGE_REF) tag first (some registries silently keep old content on tag reuse otherwise)"
	@$(CRANE_TOOL) delete $(IMAGE_REF) 2>&1 | grep -v 'MANIFEST_UNKNOWN\|manifest unknown' || true
endif
	@echo "==> Running: $(CONTAINER_TOOL) buildx build --platform=linux/$(ARCH) $(if $(filter true,$(PUSH)),--push,--load) -t $(IMAGE_REF) --build-arg BINARY=$(BINARY) --build-arg VERSION=$(VERSION) --build-arg GIT_SHA=$(GIT_SHA) $(BASE_IMAGE_BUILD_ARG) -f Dockerfile ."
	@rcfile=$$(mktemp); \
	{ $(CONTAINER_TOOL) buildx build --platform=linux/$(ARCH) \
	  $(if $(filter true,$(PUSH)),--push,--load) \
	  -t $(IMAGE_REF) \
	  --build-arg BINARY=$(BINARY) \
	  --build-arg VERSION="$(VERSION)" \
	  --build-arg GIT_SHA="$(GIT_SHA)" \
	  $(BASE_IMAGE_BUILD_ARG) \
	  -f Dockerfile . 2>&1; echo $$? > "$$rcfile"; } \
	  | grep -v '^<jemalloc>:'; \
	rc=$$(cat "$$rcfile"); rm -f "$$rcfile"; exit $$rc
	@echo "==> Done: $(if $(filter true,$(PUSH)),pushed,loaded) $(IMAGE_REF)"

# Unlike `docker-image`, this target does NOT depend on build-linux-*:
# it expects binaries/<arch>/{$(BINARY),rootfs/} to already exist for
# every platform in $(PLATFORMS). CI stages those from the per-arch
# build matrix artifacts, which is the only way to get a multi-arch
# manifest without cross-compiling or emulating (ADR-0006).
docker-image-prestaged: ## Build+push $(IMAGE_REF) for $(PLATFORMS) from ALREADY-staged binaries/ (no compile); writes $(DOCKER_METADATA_FILE)
	@echo "==> Building $(IMAGE_REF) for $(PLATFORMS) from pre-staged binaries/ (PUSH=$(PUSH))"
	@for p in $$(echo "$(PLATFORMS)" | tr ',' ' '); do \
		a=$${p#linux/}; \
		if [ ! -x "binaries/$$a/$(BINARY)" ]; then \
			echo "ERROR: binaries/$$a/$(BINARY) missing or not executable — stage it before calling this target"; exit 1; \
		fi; \
		if [ ! -d "binaries/$$a/rootfs" ]; then \
			echo "ERROR: binaries/$$a/rootfs/ missing — the TSS runtime libraries must be staged alongside the binary"; exit 1; \
		fi; \
	done
	@$(CONTAINER_TOOL) buildx build --platform=$(PLATFORMS) \
	  $(if $(filter true,$(PUSH)),--push,--load) \
	  --metadata-file "$(DOCKER_METADATA_FILE)" \
	  $(if $(filter true,$(PUSH)),--provenance=true --sbom=true,) \
	  -t $(IMAGE_REF) \
	  --build-arg BINARY=$(BINARY) \
	  --build-arg VERSION="$(VERSION)" \
	  --build-arg GIT_SHA="$(GIT_SHA)" \
	  $(BASE_IMAGE_BUILD_ARG) \
	  -f Dockerfile .
	@echo "==> Digest: $$($(MAKE) --no-print-directory docker-digest)"

docker-digest: ## Print the image digest recorded in $(DOCKER_METADATA_FILE)
	@if [ ! -f "$(DOCKER_METADATA_FILE)" ]; then echo "ERROR: $(DOCKER_METADATA_FILE) not found (run 'make docker-image-prestaged PUSH=true')" >&2; exit 1; fi
	@python3 -c "import json,sys; d=json.load(open('$(DOCKER_METADATA_FILE)')); v=d.get('containerimage.digest'); sys.exit('no containerimage.digest in $(DOCKER_METADATA_FILE)') if not v else print(v)"

docker-push: ## Push $(IMAGE_REF)
	$(CONTAINER_TOOL) push $(IMAGE_REF)

deploy-test: ## Copy a sceau binary+libs onto test nodes over SSH (HOSTS="user@host1 user@host2" required; testing only, not the eventual sysext path). Linux: reuses `make build-linux-$(ARCH)`'s local staging. macOS: pulls+extracts IMAGE_REF instead (set IMAGE_REF=...) — Mac can't produce a local Linux binary directly.
	@ARCH_NAME=$(ARCH) BINARY=$(BINARY) scripts/deploy-test-nodes.sh
