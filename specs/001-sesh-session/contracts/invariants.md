# Contract: invariant suite (`sesh verify`)

Each invariant names the principle it verifies. "Inside" means run in a
sandbox through `sbx exec`; "host" means run by sesh on the Mac. No
invariant depends on which harness a recipe runs: `sesh verify` uses a
minimal probe recipe (a shell) plus, for the snapshot checks, each
installed recipe's import rules. Every invariant must be seen to FAIL
when its protection is removed (SC-003); the removal is in the last
column.

## Network (Principle II)

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-N1 | Inside: `curl https://example.com` | refused | allowing `example.com` |
| INV-N2 | Inside: reach a declared local endpoint (a probe server started by verify) | 200 | removing that endpoint |
| INV-N4 | Inside: raw TCP to a public IP:443 bypassing the proxy; UDP 53 to 1.1.1.1 | refused | n/a (runtime guarantee; recorded) |
| INV-N5 | Inside: reach a LAN address and an undeclared host loopback port | refused | allowing that port |
| INV-N6 | Inside: `DELETE` on a GitHub repo path that doesn't exist | refused by the sbx proxy (not a GitHub 404) | removing the deny rule |
| INV-N7 | Host+inside: a repository with `only = [probe]` can't reach a globally allowed endpoint | refused | ignoring `only` |
| INV-N8 | Host: every local endpoint and shim port is bound to loopback only | true | binding the probe server to `0.0.0.0` |
| INV-N9 | Host: a host refused during a probe session appears in the refused-host report | listed | dropping the report |

## Secrets (Principle III)

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-S1 | Inside: collect `env`, `/proc/*/environ`, `/proc/*/cmdline` and an archive of the sandbox home. Host: search those and the workspace for the real test-credential value (the value never goes into the sandbox to be searched for) | not found | exporting the real value as env |
| INV-S2 | Inside: request to a host echo server bound to the test credential, using its standard header | upstream receives the real value; response doesn't contain it | n/a |
| INV-S3 | Inside: same as S2 with the placeholder in a made-up header (#707) | real value never appears in the response | expected to leak while #707 is open: **KNOWN-RISK** |
| INV-S4 | Inside: send the placeholder to a host the credential is NOT bound to | that host receives only the placeholder | binding the credential to that host |
| INV-S5 | Host: grep sesh's config, state dir and logs for the real test value | not found | logging the value |
| INV-S6 | Host: build a snapshot from a fixture home that has a recipe credential file, one planted value per scan rule, and a symlink to a fake `~/.ssh` | none of them present in the snapshot; each reported by path and reason | disabling the scan / following symlinks |

## Filesystem and host execution (Principles I, IV)

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-F1 | Inside: read `~/.ssh`, `~/Library`, the host checkout path and the real harness config path | not accessible | mounting home |
| INV-F2 | Inside: write outside the workspace, the sandbox home and temp dirs | fails | n/a (runtime guarantee) |
| INV-H1 | Inside: plant `.git/hooks/post-checkout`, `core.fsmonitor`, `.envrc`, a Makefile target, a `.claude/settings.json` hook; commit. Host: return work, then `git status`, `git checkout`, `git log` | no planted command runs on the host (sentinel absent); the summary lists `.envrc`, `Makefile`, `.claude/settings.json` as risky | mounting the host checkout directly |
| INV-H2 | Host: the host checkout's `.git` is not mounted into the sandbox | true | mounting it |
| INV-H3 | Inside: modify the imported harness config (add a hook). Host: the fixture host config is byte-identical afterwards, and the next session's snapshot doesn't contain the change | true | syncing the snapshot back |

## Policy integrity (Principle V)

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-P1 | Host: rules applied in sbx == rules compiled from config and recipe | identical | adding a rule by hand with `sbx policy` |
| INV-P2 | Host: a repo containing `.sesh.toml`, `.sesh/recipes/*.toml`, harness config dirs and an `AGENTS.md` asking for more access compiles the same policy as an empty repo, and no repo recipe is listed by `sesh recipes` | identical | reading repo-local config |
| INV-P3 | Host: sbx is at or above the pinned minimum | true | lowering the pin |
| INV-P4 | Host: with no session open, no `sesh` process is running, and every `sesh-shim` process is a launchd job | true | leaving a `sesh` process in the background |

## Runtime extras

| ID | Check | Expected | Seen failing by |
|---|---|---|---|
| INV-R1 | Inside: `docker run --rm hello-world` | succeeds using the sandbox's own engine | n/a |
| INV-R2 | Inside: the host's Docker socket isn't reachable | true | mounting it |

## Result states

- **PASS** / **FAIL**: as expected or not.
- **KNOWN-RISK**: a documented upstream gap (currently only INV-S3). It
  doesn't fail the run but is always printed with its issue link.

SC-003's "100% pass" counts PASS and KNOWN-RISK.

The test credential, echo server and probe server are created by
`sesh verify` for the run and removed afterwards. The test credential is
a random value stored under its own Keychain item. INV-N3 (search) was
retired with ADR 0004; IDs are not reused.
