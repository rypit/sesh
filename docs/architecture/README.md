# Architecture diagram

`docs/architecture.svg`, shown in the top-level README and in the
[Solutions Architecture](solutions-architecture.md), is generated from
`diagram.jsx` in this folder. `diagram.jsx` is the diagram's source.

## When to update it

Whenever a change adds, removes or renames a component, moves one
between zones, or changes what talks to what. The diagram, the Solutions
Architecture text and the code change together, in the same pull
request.

## How

1. Change `diagram.jsx`.
2. Run `python3 scripts/render-architecture.py`.
3. Update `solutions-architecture.md` to match.
4. Commit `diagram.jsx`, `docs/architecture.svg` and the document
   together.

CI runs `python3 scripts/render-architecture.py --check` and fails if the
SVG doesn't match `diagram.jsx`.

The script supports the JSX the widget uses: string constants, small
helper functions such as `B(x, y, w, h, accent)`, and attribute
expressions that are literals, names or `a ? b : c`. Theme colours map
to the light and dark palette at the top of the script.
