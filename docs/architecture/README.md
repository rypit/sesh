# Architecture diagram

`docs/architecture.svg`, shown in the top-level README, is generated from
`diagram.jsx` in this folder. That file is the diagram widget from the
[sesh Solutions Architecture](https://claude.ai/artifact/Hq9kuLsqLakFbAUW1Jyt5T)
document, copied verbatim. The document defines sesh; this copy keeps
the README's picture identical to it.

## When to update it

Whenever a change adds, removes or renames a component, moves one
between zones, or changes what talks to what. The document and the repo
change together, in the same pull request.

## How

1. Change the diagram in the Solutions Architecture document (or, if
   you change `diagram.jsx` first, copy the result back into the
   document's widget so the two stay identical).
2. Copy the widget's code into `diagram.jsx`.
3. Run `python3 scripts/render-architecture.py`.
4. Commit `diagram.jsx` and `docs/architecture.svg` together.

CI runs `python3 scripts/render-architecture.py --check` and fails if the
SVG doesn't match `diagram.jsx`.

The script supports the JSX the widget uses: string constants, small
helper functions such as `B(x, y, w, h, accent)`, and attribute
expressions that are literals, names or `a ? b : c`. Theme colours map
to the light and dark palette at the top of the script.
