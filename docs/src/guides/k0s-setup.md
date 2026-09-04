# k0s Setup

Wire a k0s control plane to sceau so that Secrets (and any other resources
you choose) are encrypted at rest with TPM-sealed DEKs. k0s runs
kube-apiserver as a **host process**, so a plain unix socket on the host
works — no sidecar, no static pod.

This guide assumes sceau is already installed and running on the host (see
[Kairos Deployment](kairos-deployment.md)) and that you can edit `k0s.yaml`
and restart the controller.

## 1. Start sceau

Confirm the socket exists and is root-only:

```sh
systemctl status sceau
ls -l /run/sceau/sceau.sock
# srw------- 1 root root ... /run/sceau/sceau.sock
```

## 2. Create the EncryptionConfiguration

`/var/lib/k0s/encryption.conf`:

```yaml
apiVersion: apiserver.config.k8s.io/v1
kind: EncryptionConfiguration
resources:
  - resources: ["secrets"]
    providers:
      - kms:
          apiVersion: v2
          name: sceau
          endpoint: unix:///run/sceau/sceau.sock
          timeout: 3s
      - identity: {}
```

Two details that matter:

- **`apiVersion: v2`** — must match the proto sceau serves. Do not copy
  older `v1` examples from the internet.
- **`identity: {}` listed *last*** — providers are tried in order on write
  and in any order on read. Keeping identity as a fallback means *existing
  plaintext Secrets stay readable* until you migrate them (step 4). New
  writes go to `kms` (the first provider) immediately.

## 3. Point k0s at it

`k0s.yaml`:

```yaml
spec:
  api:
    extraArgs:
      encryption-provider-config: /var/lib/k0s/encryption.conf
```

Apply and restart the controller so the apiserver picks up the flag:

```sh
k0s stop
k0s start
```

Verify the plugin is healthy from the apiserver's point of view — encrypt a
Secret and check the stored form:

```sh
kubectl create secret generic kms-check --from-literal=key=value
kubectl get secret kms-check -o yaml   # readable through the API

# On the host, inspect the raw etcd value:
k0s etcd member-list   # sanity
k0s kubectl get --raw /api/v1/namespaces/default/secrets/kms-check \
  | head -c 200
```

The raw value in etcd must start with `k8s:enc:kms:v2:sceau:` — anything
else (notably a missing prefix, i.e. plaintext) means the config did not
take effect.

## 4. Migrate existing Secrets

New writes are encrypted, but Secrets written *before* step 2 are still
plaintext in etcd (readable via the `identity` fallback). Re-encrypt them
with the standard KMS migration procedure — a no-op rewrite of every Secret
forces a re-encrypt through the now-first `kms` provider:

```sh
kubectl get secrets --all-namespaces -o json | kubectl replace -f -
```

Once the replace completes and you have verified cluster health, remove the
fallback so plaintext is never written again:

```yaml
resources:
  - resources: ["secrets"]
    providers:
      - kms:
          apiVersion: v2
          name: sceau
          endpoint: unix:///run/sceau/sceau.sock
          timeout: 3s
```

then restart k0s again.

!!! danger "Removing identity before migrating breaks reads"
    If you drop `identity: {}` before the `kubectl replace` migration, every
    pre-existing plaintext Secret becomes unreadable. Migrate first, verify,
    then tighten.

## Rotating to or from another provider

The same mechanism covers every provider transition — `aescbc` → `kms`,
`kms` → a different KMS, or `kms` → plaintext:

1. Edit `encryption.conf` so the **new** provider is first and the **old**
   one is still listed (order defines the write path; every listed provider
   can read).
2. Restart k0s.
3. `kubectl get secrets --all-namespaces -o json | kubectl replace -f -`.
4. Remove the old provider and restart once more.

To encrypt additional resource types (`configmaps`, CRs, …), add a new
`resources:` entry — each entry has its own provider list.

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| apiserver fails to start, KMS health check errors | sceau not running or socket path mismatch — `systemctl status sceau`, compare with `endpoint:` |
| `unknown key_id ... this TPM only serves ...` on Decrypt | The TPM was cleared/replaced, or the ciphertext came from a different host. See the [threat model](../concepts/threat-model.md). |
| Secret writes hang ~3s then error | The `timeout: 3s` in the provider config is expiring — check sceau logs (`journalctl -u sceau`) for TPM errors. |
| etcd values lack the `k8s:enc:kms:v2:` prefix | `encryption-provider-config` flag not applied — check `k0s.yaml` and that k0s was restarted. |
