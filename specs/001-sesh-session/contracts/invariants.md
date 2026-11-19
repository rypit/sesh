# Contract: invariant suite (`sesh verify`)

Each invariant names the principle it verifies. "Inside" means run in the
repo's sandbox through `sbx exec`; "host" means run by Sesh on the Mac.
Every invariant must be seen to FAIL when its protection is removed
(SC-002); the removal is noted in the last column.

## Network

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-N1 | Inside: `curl https://example.com` | refused | allowing `example.com` |
| INV-N2 | Inside: `curl http://host.docker.internal:<engine.port>/v1/models` | 200 | removing the engine rule |
| INV-N3 | Inside: SearXNG JSON query (online profile) | 200 with results | removing the search rule |
| INV-N4 | Inside: raw TCP to a public IP:443 bypassing the proxy; UDP 53 to 1.1.1.1 | refused | n/a (runtime guarantee; recorded) |
| INV-N5 | Inside: reach a LAN address (e.g. router IP) and an un-allowed host loopback port | refused | allowing that port |
| INV-N6 | Inside: `DELETE` on a GitHub repo path that doesn't exist, so a policy failure is harmless | refused by the sbx proxy (not a GitHub 404) | removing the deny rule |
| INV-N7 | Offline profile: N3 and registry access | refused | running in the online profile |

## Secrets

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-S1 | Inside: scan `env`, `/proc/*/environ`, `/proc/*/cmdline` and the workspace for the real test-credential value | not found | exporting the real value as env |
| INV-S2 | Inside: request to a host echo server bound to the test credential, using its standard header | upstream receives the real value; response doesn't contain it | n/a |
| INV-S3 | Inside: same as S2, but the placeholder sits in a made-up header (the #707 case) | real value never appears in the response | expected to leak on sbx while #707 is open: reported as **KNOWN-RISK**, not PASS, until fixed upstream |
| INV-S4 | Inside: send the placeholder to a host the credential is NOT bound to | that host receives only the placeholder | binding the credential to that host |
| INV-S5 | Host: grep Sesh's config, state dir and logs for the real test value | not found | logging the value |

## Filesystem and host execution

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-F1 | Inside: read `~/.ssh`, `~/Library` and the host checkout path | not accessible | mounting home |
| INV-F2 | Inside: write outside the workspace and temp dirs | fails | n/a (runtime guarantee) |
| INV-H1 | Inside: plant `.git/hooks/post-checkout`, `core.fsmonitor`, `.envrc`, a Makefile target; commit. Host: `sesh pull`, then `git status`, `git checkout`, `git log` in the host checkout | no planted command runs on the host (sentinel file absent) | mounting the host checkout directly |
| INV-H2 | Host: the host checkout's `.git` is not mounted into the sandbox | true | mounting it |

## Policy integrity

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-P1 | Host: rules applied in sbx == rules compiled from config | identical | adding a rule by hand with `sbx policy` |
| INV-P2 | Host: a repo containing `.sesh.toml`, a pi `models.json` and an `AGENTS.md` asking for more access gets the same compiled policy as an empty repo | identical | reading repo-local config |
| INV-P3 | Host: sbx, Splash and pi versions are at or above the pinned minimums | true | lowering the pin |

## Runtime extras

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-R1 | Inside: `docker run --rm hello-world` | succeeds using the sandbox's own engine | n/a |
| INV-R2 | Inside: the host's Docker socket isn't reachable | true | mounting it |

## Result states

- **PASS** / **FAIL**: as expected or not.
- **KNOWN-RISK**: a documented upstream gap (currently only INV-S3). It
  doesn't fail the run, but is always printed with its issue link. Any
  other invariant that can't pass is a FAIL.

SC-002's "100% pass" counts PASS and KNOWN-RISK.

The test credential and echo server are created by `sesh verify` for the
run and removed afterwards. The test credential is a random value stored
under its own Keychain item.
