#!/bin/sh
# Copyright (c) 2026 Erick Bourgeois, sceau
# SPDX-License-Identifier: Apache-2.0
#
# Get a sceau binary + its TPM2-TSS shared libs + its systemd unit
# (contrib/systemd/$BINARY.service, if present) onto one or more test nodes
# over SSH — for manual testing before this is packaged as a Kairos sysext
# (see docs/migration-ha-existing-cluster.md). Testing convenience only, not
# the eventual node-provisioning path. The unit is installed to
# /etc/systemd/system/ and `daemon-reload`ed, but never enabled/started --
# that's left to the operator (see the final echo each host prints).
#
# Two source modes, chosen automatically by `uname -s` (matches this
# Makefile's own build-linux-* split: native cross-compile on Linux,
# sandboxed-Docker fallback on Mac, since Docker Desktop's Linux VM is the
# only way a Mac produces a real Linux binary at all):
#
#   Linux — reuse the local build directly. `make build-linux-$ARCH_NAME`
#     already stages binaries/$ARCH_NAME/sceau and
#     binaries/$ARCH_NAME/rootfs/usr/lib/$TRIPLE/libtss2*.so.* on disk; no
#     image involved, nothing to pull.
#   macOS (Darwin) — there is no local Linux binary to reuse (Mac can't
#     produce one directly), so pull IMAGE_REF and extract the binary + libs
#     from its filesystem instead, via `docker create`/`docker cp` (no
#     `crane` dependency — `docker` is already required by this Makefile's
#     own docker-image target). In-image paths (/usr/local/bin/sceau,
#     /usr/lib/<triple>) come straight from Dockerfile's own COPY
#     destinations — see that file if these ever drift apart.
#
# Never hardcode a real image reference or hostname here
# (rules/no-real-infrastructure.md — this is a public OSS repo). Both always
# come from the caller at runtime.
#
# Usage (Linux, after `make build-linux-amd64`):
#   HOSTS="user@host1 user@host2" scripts/deploy-test-nodes.sh
#
# Usage (macOS):
#   IMAGE_REF="ghcr.io/firestoned/sceau:v0.1.0" \
#     HOSTS="user@host1 user@host2" \
#     scripts/deploy-test-nodes.sh
#
# SOURCE_MODE=local|image overrides the uname-based auto-detection.

set -eu

: "${HOSTS:?set HOSTS to a space-separated list of user@host targets, e.g. HOSTS=\"kairos@node1 kairos@node2\"}"
ARCH_NAME="${ARCH_NAME:-amd64}"
BINARY="${BINARY:-sceau}"
SSH_KEY="${SSH_KEY:-}"
REMOTE_DIR="${REMOTE_DIR:-/opt/sceau}"
CONTAINER_TOOL="${CONTAINER_TOOL:-docker}"

case "$ARCH_NAME" in
  amd64) triple=x86_64-unknown-linux-gnu; sys_triple=x86_64-linux-gnu ;;
  arm64) triple=aarch64-unknown-linux-gnu; sys_triple=aarch64-linux-gnu ;;
  *)
    echo "error: unknown ARCH_NAME '$ARCH_NAME' (expected amd64 or arm64)" >&2
    exit 1
    ;;
esac
# `triple` (the Rust target triple, e.g. x86_64-unknown-linux-gnu) is the
# local staging directory name `make build-linux-$ARCH_NAME` uses (mirrors
# the Makefile's own native-build path) — correct for SOURCE_MODE=local.
# `sys_triple` (the real Debian/dpkg multiarch triplet, e.g.
# x86_64-linux-gnu — no "-unknown-" vendor field) is where the TSS libs
# actually live *inside the built container image* (staged there by
# build-linux-*'s container-fallback branch via `gcc -dumpmachine`, or by
# the distroless base's own expected ld.so layout) — required for
# SOURCE_MODE=image's `docker cp`. These are two different strings for
# the same architecture; using the wrong one for either mode fails with
# "Could not find the file" from `docker cp`, not a helpful error.

if [ -z "${SOURCE_MODE:-}" ]; then
  case "$(uname -s)" in
    Darwin) SOURCE_MODE=image ;;
    *) SOURCE_MODE=local ;;
  esac
fi

repo_root=$(cd "$(dirname "$0")/.." && pwd)
unit_path="$repo_root/contrib/systemd/$BINARY.service"

work_dir=""
cleanup() {
  [ -n "$work_dir" ] && rm -rf "$work_dir"
  [ -n "${container_id:-}" ] && "$CONTAINER_TOOL" rm -f "$container_id" >/dev/null 2>&1 || true
}
trap cleanup EXIT

case "$SOURCE_MODE" in
  local)
    bin_path="$repo_root/binaries/$ARCH_NAME/$BINARY"
    lib_dir="$repo_root/binaries/$ARCH_NAME/rootfs/usr/lib/$triple"
    if [ ! -f "$bin_path" ] || [ ! -d "$lib_dir" ]; then
      echo "error: $bin_path / $lib_dir not found — run 'make build-linux-$ARCH_NAME' first" >&2
      exit 1
    fi
    ;;
  image)
    : "${IMAGE_REF:?SOURCE_MODE=image requires IMAGE_REF, e.g. IMAGE_REF=ghcr.io/firestoned/sceau:v0.1.0}"
    work_dir=$(mktemp -d "${TMPDIR:-/tmp}/sceau-deploy.XXXXXX")
    mkdir -p "$work_dir/lib"

    echo "==> pulling $IMAGE_REF (linux/$ARCH_NAME)"
    "$CONTAINER_TOOL" pull --platform "linux/$ARCH_NAME" "$IMAGE_REF"

    echo "==> extracting binary + libs from $IMAGE_REF"
    # --platform here too: `create` re-resolves the reference and would
    # otherwise silently prefer a locally-cached native-arch layer if one
    # existed, independent of what was just pulled.
    container_id=$("$CONTAINER_TOOL" create --platform "linux/$ARCH_NAME" "$IMAGE_REF")
    "$CONTAINER_TOOL" cp "$container_id:/usr/local/bin/$BINARY" "$work_dir/$BINARY"
    "$CONTAINER_TOOL" cp "$container_id:/usr/lib/$sys_triple" "$work_dir/lib/$sys_triple"
    "$CONTAINER_TOOL" rm -f "$container_id" >/dev/null
    container_id=""
    chmod 0755 "$work_dir/$BINARY"

    bin_path="$work_dir/$BINARY"
    lib_dir="$work_dir/lib/$sys_triple"
    ;;
  *)
    echo "error: SOURCE_MODE must be 'local' or 'image', got '$SOURCE_MODE'" >&2
    exit 1
    ;;
esac

ssh_opts=""
scp_opts=""
if [ -n "$SSH_KEY" ]; then
  ssh_opts="-i $SSH_KEY"
  scp_opts="-i $SSH_KEY"
fi

service_name=$(basename "$unit_path" .service)

for host in $HOSTS; do
  echo "==> $host: staging $REMOTE_DIR"
  # shellcheck disable=SC2086
  ssh $ssh_opts "$host" "sudo mkdir -p $REMOTE_DIR/bin $REMOTE_DIR/lib && sudo chown \$(id -u):\$(id -g) $REMOTE_DIR/bin $REMOTE_DIR/lib"

  # A running $BINARY holds its own executable open -- overwriting it via
  # scp fails with ETXTBSY, which scp's SFTP protocol reports as a bare
  # "Failure" with no further detail (confirmed live, 2026-09-06). Stop it
  # first if active, and only restart afterward if it actually was.
  was_active=false
  # shellcheck disable=SC2086
  if [ "$(ssh $ssh_opts "$host" "systemctl is-active $service_name 2>/dev/null || true")" = "active" ]; then
    was_active=true
    echo "==> $host: stopping $service_name (currently running, would block the copy)"
    # shellcheck disable=SC2086
    ssh $ssh_opts "$host" "sudo systemctl stop $service_name"
  fi

  echo "==> $host: copying $BINARY"
  # shellcheck disable=SC2086
  scp $scp_opts "$bin_path" "$host:$REMOTE_DIR/bin/$BINARY"

  echo "==> $host: copying TPM2-TSS libs"
  # shellcheck disable=SC2086
  scp $scp_opts "$lib_dir"/*.so* "$host:$REMOTE_DIR/lib/"

  echo "==> $host: setting permissions"
  # The lib glob must expand on the REMOTE shell, never the local one — the
  # whole command is built as one local string (safe: $REMOTE_DIR/$BINARY
  # are plain values, no glob characters) and handed to ssh as a single
  # argument, so the local shell never sees an unquoted `*`.
  remote_cmd="sudo chmod 0755 $REMOTE_DIR/bin/$BINARY && sudo chmod 0644 $REMOTE_DIR/lib/"'*.so*'
  # shellcheck disable=SC2086
  ssh $ssh_opts "$host" "$remote_cmd"

  if [ -f "$unit_path" ]; then
    echo "==> $host: copying $(basename "$unit_path") to /etc/systemd/system/"
    # shellcheck disable=SC2086
    scp $scp_opts "$unit_path" "$host:$REMOTE_DIR/bin/$(basename "$unit_path")"
    # shellcheck disable=SC2086
    ssh $ssh_opts "$host" "sudo mv $REMOTE_DIR/bin/$(basename "$unit_path") /etc/systemd/system/$(basename "$unit_path") && sudo systemctl daemon-reload"
  else
    echo "==> $host: no unit file at $unit_path, skipping (binary/lib only)"
  fi

  if [ "$was_active" = true ]; then
    echo "==> $host: restarting $service_name (was running before this deploy)"
    # shellcheck disable=SC2086
    ssh $ssh_opts "$host" "sudo systemctl start $service_name"
  fi

  echo "==> $host: done — LD_LIBRARY_PATH=$REMOTE_DIR/lib $REMOTE_DIR/bin/$BINARY --help"
  echo "    or: sudo systemctl enable --now $service_name (unit installed, not started)"
done
