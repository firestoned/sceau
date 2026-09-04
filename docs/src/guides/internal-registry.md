# Internal Registry

Build the distroless image and push it to an **internal registry mirror** in
one `make` invocation — the pattern for air-gapped sites, bandwidth-limited
edge locations, or policies that forbid pulling from public registries at
provision time.

All of the knobs are Makefile variables (see `make help`); the workflow below
composes them.

## The pattern

```sh
make docker-image \
  ARCH=amd64 \
  PUSH=true \
  BASE_IMAGE=registry.internal:5000/distroless/cc-debian13:nonroot \
  REGISTRY=registry.internal:5000/platform \
  ORG= \
  IMAGE=v0.1.0
```

What each variable does:

| Variable | Value in the example | Effect |
| --- | --- | --- |
| `ARCH` | `amd64` | Cross-builds the Linux binary + stages the TSS libraries via `build-linux-amd64` (a `rust:1-bookworm` container; nothing compiles inside the image build). |
| `PUSH` | `true` | `buildx --push` instead of `--load`. |
| `BASE_IMAGE` | internal mirror of distroless | The image build never touches `gcr.io` — your mirror supplies the pinned base. |
| `REGISTRY` | `registry.internal:5000/platform` | Registry host, optionally with a namespace path. |
| `ORG` | *(empty)* | With `ORG=` empty, the org segment is omitted from the reference (banlieue pattern). |
| `IMAGE` | `v0.1.0` | Tag; alias of `IMAGE_TAG`. |

The resulting reference is:

```text
registry.internal:5000/platform/sceau:v0.1.0
   └────── REGISTRY ──────┘   BINARY  └ IMAGE ┘
```

## Gotcha: a registry host needs a dot or a port

Docker decides whether the first path segment is a registry host or a Docker
Hub repository name **lexically**: the segment must contain a `.` or a `:`,
or be `localhost`. Everything else is silently treated as Docker Hub.

```text
registry.internal:5000/platform/sceau:v0.1.0   ✅ registry host (has a port)
mirror.foo.io/platform/sceau:v0.1.0            ✅ registry host (has dots)
registry/platform/sceau:v0.1.0                 ❌ docker.io/registry/platform/... (!!)
```

If your "push" inexplicably fails with a Docker Hub auth error, this is why.
Use the host's FQDN or append its port.

## Escape hatch: IMAGE_REF

When the composed `REGISTRY[/ORG]/BINARY:TAG` shape does not fit — e.g. the
mirror mandates a specific repository layout — override the full reference
directly:

```sh
make docker-image \
  ARCH=amd64 \
  PUSH=true \
  IMAGE_REF=registry.internal:5000/mirrors/firestoned/sceau:v0.1.0-mirror1
```

`IMAGE_REF` is a `?=` variable, so the environment wins and the composition
logic is bypassed entirely.

## Verifying what you built

The pushed image is the same distroless artifact CI produces for
`ghcr.io/firestoned/sceau`: the prebuilt binary, the staged TSS runtime
libraries, OCI labels carrying `VERSION` and `GIT_SHA`, and a pinned base.
Sanity-check before rolling it into a Kairos image build:

```sh
docker run --rm --entrypoint /usr/local/bin/sceau \
  registry.internal:5000/platform/sceau:v0.1.0 --help
```

(Running it for real requires a TPM device — `--help` is the distroless-safe
smoke test.)

## Using the mirrored image

Reference the mirror wherever the public image would appear — for example in
the Kairos `COPY --from` stage of the
[Kairos deployment guide](kairos-deployment.md#bundling-into-a-kairos-image):

```dockerfile
FROM registry.internal:5000/platform/sceau:v0.1.0 AS sceau
```
