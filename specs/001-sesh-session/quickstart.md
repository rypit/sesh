# Quickstart and validation guide

How feature 001 should feel when done, written as runnable acceptance
walkthroughs for US1–US6. Command details are in
[contracts/cli.md](./contracts/cli.md), config in
[contracts/config.md](./contracts/config.md), recipes in
[contracts/recipe.md](./contracts/recipe.md), and the checks behind
`sesh verify` in [contracts/invariants.md](./contracts/invariants.md).

## Prerequisites

- A Mac on Apple Silicon at the macOS version sbx requires.
- Homebrew.
- At least one harness you already use (Claude Code, Codex CLI,
  opencode or pi), with an API key for its model provider, or a local
  inference server.

## 1. Install and set up once (US6, SC-008)

```bash
cargo install --locked --git https://github.com/rypit/sesh   # later: brew install rypit/tap/sesh
sesh init
```

`sesh init` checks the Mac, installs and logs in to sbx, asks for a
default recipe and model endpoint, stores the key in the Keychain
(`security add-generic-password` under `sesh/<name>`), and writes
`~/.config/sesh/config.toml`.

**Expect**: `sesh doctor` passes. Running `sesh init` again changes
nothing. On an unsupported Mac, `sesh init` stops before changing
anything and names the unmet requirement (exit 3).

## 2. Run your harness, sandboxed (US1, SC-001, SC-002)

```bash
cd ~/code/some-repo
sesh claude            # or: sesh codex / sesh opencode / sesh pi / sesh
```

**Expect**:
- The harness opens with your own settings, skills and tool config, and
  answers through your model endpoint. No edits to your harness config.
- Ask it to `cat ~/.ssh/id_ed25519` or read your real `~/.claude`: the
  read fails.
- Ask it to add a hook to its settings, then exit. Your real harness
  config on the Mac is unchanged, and the hook is gone next session.
- Run `sesh claude` again: it opens in under 10 s, with the previous
  work and installed dependencies still there.
- `sesh claude --resume` passes `--resume` to the harness unchanged.

## 3. Get the work back (US2, SC-005, SC-006)

Have the agent commit some code, plus a change to `.envrc` and
`Makefile`, then exit the harness.

**Expect**: within 5 s, a summary like

```text
Returned 3 commits to sesh/20261120-1/main
  carried changes: 1 commit (your uncommitted edits at start)
  risky paths: .envrc, Makefile   (can run code on your Mac; review first)
Refused hosts this session:
  statsig.anthropic.com (12)    sesh allow statsig.anthropic.com
```

```bash
git log sesh/20261120-1/main
git diff main sesh/20261120-1/main
git merge sesh/20261120-1/main      # when you're happy
```

Your current branch and working tree are untouched, and no planted hook
or `.envrc` ran (`sesh verify --only INV-H1` checks this).

## 4. Grant access when something was blocked (US3, SC-007)

```bash
sesh allow docs.rs              # this repo only
sesh allow docs.rs --global     # every repo
sesh policy show                # every endpoint, with where it came from
```

**Expect**: the next session reaches `docs.rs`. `sesh policy show` lists
it with source `repo:<path>` or `global`, and shows credential bindings
by name and host only.

For a sensitive repository, restrict it to a local model in your
config (`repo."<path>".only = ["local-llm"]`) and point the recipe at
it (`recipe.claude.model_endpoint = "local-llm"`).

## 5. Verify (US4, SC-003, SC-004)

```bash
sesh verify
```

**Expect**: every invariant PASS (INV-S3 KNOWN-RISK while
docker/sbx-releases#707 is open), exit 0. Allow `example.com` globally
and rerun: INV-N1 FAILs with expected vs observed, exit 4.

## 6. Bring another harness (US5)

Write `~/.config/sesh/recipes/<name>.toml` following
[contracts/recipe.md](./contracts/recipe.md), then:

```bash
sesh recipes show <name>
sesh <name>
```

**Expect**: it runs as in step 2. A recipe placed inside the repository
(`.sesh/recipes/`) is ignored, and sesh says so.

## Housekeeping

```bash
sesh ls                  # sandboxes, sessions, unreturned commits
sesh reset               # discard this repo's sandbox (lists unreturned work first)
sesh doctor --probe      # also test the model endpoint with one tool call
```

## Things to know

- Don't open sesh's isolated workspace (`~/.local/state/sesh/…`) in your
  editor or run its scripts on your Mac; review returned branches in
  your own checkout.
- Review files flagged as risky before running anything from them.
- Your code goes to the endpoints you allowed. Choose them per
  repository when it matters.
