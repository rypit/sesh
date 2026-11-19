# Quickstart (target experience)

This is how Sesh should feel when feature 001 is done. It's the
acceptance walkthrough for US1–US5.

## Requirements

- Apple Silicon M3 or later, macOS 26.4+, 48 GB+ memory recommended.
- Homebrew.

## Install and set up once

```bash
uv tool install git+https://github.com/rypit/sesh   # later: brew install rypit/tap/sesh
sesh init
```

`sesh init` checks your Mac, then installs and sets up the components:

- Splash, plus the default model (this is the long download),
- the SearXNG image,
- sbx, including login,
- the pi kit.

It writes a default `~/.config/sesh/config.toml` if you don't have one.
Running it again changes nothing.

Optional, for GitHub access. Use a fine-grained token limited to the
repos you work on, with no gist or admin permissions:

```bash
security add-generic-password -s sesh/github -a "$USER" -w   # paste the token at the prompt
```

## Daily use

```bash
git clone https://github.com/you/project && cd project
sesh                  # stack up -> sandbox -> pi, talking to your local model
```

Work with the agent. It edits, builds and tests inside the sandbox. It
can search the web and install packages. If you set up the GitHub token,
it can push branches, but it never sees the token.

```bash
sesh pull             # agent's commits arrive as branch sesh/<session>/*
git log sesh/<session>/main
git diff main sesh/<session>/main
git merge sesh/<session>/main     # when you're happy
```

## Checking and housekeeping

```bash
sesh verify           # every security invariant, PASS/FAIL
sesh policy show      # what this repo's sandbox can reach
sesh --profile offline   # local model only, no internet at all
sesh ls               # sandboxes and sessions
sesh reset            # discard this repo's sandbox (lists unpulled work first)
sesh stack down       # stop the engine and search
```

## Things to know

- Don't open Sesh's isolated workspace (`~/.local/state/sesh/…`) in your
  editor or run its scripts on your Mac. Bring work back with
  `sesh pull` and review it in your own checkout.
- Run the project's tests in the sandbox, not on your Mac, until you've
  reviewed the agent's changes.
- Web searches leave your machine through SearXNG's upstream engines. Use
  `--profile offline` for sensitive code.
