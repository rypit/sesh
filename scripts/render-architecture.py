#!/usr/bin/env python3
"""Render docs/architecture/diagram.jsx to docs/architecture.svg.

The diagram's source is the JSX widget from the sesh Solutions
Architecture document, kept verbatim in docs/architecture/diagram.jsx.
This script turns it into a static SVG that GitHub can show in the
README, with light and dark colours.

    python3 scripts/render-architecture.py          # write the SVG
    python3 scripts/render-architecture.py --check  # exit 1 if it's stale

Supported JSX subset (what the widget uses): `const name = 'value', …`
constants; `const F = (a, b) => <jsx/>` helpers called as `{F(1, 2)}`;
attribute expressions that are literals, names or `c ? a : b`.
Standard library only.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "docs" / "architecture" / "diagram.jsx"
OUTPUT = ROOT / "docs" / "architecture.svg"

# Theme tokens the widget uses, mapped to light and dark colours.
PALETTE = {
    "--cds-chart-axis": ("#6b7280", "#9ca3af"),
    "--cds-chart-grid": ("#d1d5db", "#3f4652"),
    "--cds-chart-categorical-1": ("#2563eb", "#60a5fa"),
    "--cds-chart-reference-tint": ("#f5f6f8", "#1c2129"),
    "--cds-text-primary": ("#111827", "#f3f4f6"),
    "--cds-text-secondary": ("#4b5563", "#a3abb8"),
}

KEBAB = {
    "fontSize": "font-size",
    "fontWeight": "font-weight",
    "textAnchor": "text-anchor",
    "strokeWidth": "stroke-width",
    "strokeDasharray": "stroke-dasharray",
    "markerEnd": "marker-end",
    "fillOpacity": "fill-opacity",
}
STYLE_ATTRS = {"fill", "stroke"}  # values that may hold var(); set via style


def split_top(text, sep=","):
    """Split on `sep` outside quotes and brackets."""
    parts, depth, quote, cur = [], 0, None, ""
    for ch in text:
        if quote:
            cur += ch
            if ch == quote:
                quote = None
            continue
        if ch in "'\"":
            quote = ch
        elif ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        elif ch == sep and depth == 0:
            parts.append(cur.strip())
            cur = ""
            continue
        cur += ch
    if cur.strip():
        parts.append(cur.strip())
    return parts


def evaluate(expr, env):
    expr = expr.strip()
    m = re.fullmatch(r"(\w+)\s*\?\s*(.+?)\s*:\s*(.+)", expr)
    if m:
        cond = evaluate(m.group(1), env)
        return evaluate(m.group(2) if cond not in (False, "false", 0) else m.group(3), env)
    if expr in ("true", "false"):
        return expr == "true"
    if re.fullmatch(r"-?\d+(\.\d+)?", expr):
        return expr
    m = re.fullmatch(r"'([^']*)'|\"([^\"]*)\"", expr)
    if m:
        return m.group(1) if m.group(1) is not None else m.group(2)
    if expr in env:
        return env[expr]
    raise ValueError(f"unsupported expression: {expr!r}")


def substitute(jsx, env):
    """Replace attr={expr} with attr='value', and {Helper(args)} with its JSX."""
    def call(m):
        name, args = m.group(1), split_top(m.group(2))
        params, body = env["__helpers__"][name]
        local = dict(env)
        local.update({p: evaluate(a, env) for p, a in zip(params, args)})
        return substitute(body, local)

    jsx = re.sub(r"\{(\w+)\(([^{}]*)\)\}", call, jsx)
    return re.sub(r"=\{([^{}]*)\}", lambda m: "='" + str(evaluate(m.group(1), env)) + "'", jsx)


def to_svg_attrs(jsx):
    """Rename JSX attributes, move var() colours into style, drop data-claude-*."""
    def tag(m):
        name, attrs, close = m.group(1), m.group(2), m.group(3)
        out, style = [], []
        for key, val in re.findall(r"([\w:-]+)='([^']*)'", attrs):
            if key.startswith("data-claude"):
                continue
            key = KEBAB.get(key, key)
            if key in STYLE_ATTRS and "var(" in val:
                style.append(f"{key}:{val}")
            else:
                out.append(f'{key}="{val}"')
        if style:
            out.append('style="' + ";".join(style) + '"')
        return "<" + name + ("" if not out else " " + " ".join(out)) + close

    return re.sub(r"<(\w+)((?:\s+[\w:-]+='[^']*')*)\s*(/?>)", tag, jsx)


def render(source):
    env = {"__helpers__": {}}
    for names in re.findall(r"const ((?:\w+ = '[^']*'(?:, )?)+);", source):
        for k, v in re.findall(r"(\w+) = '([^']*)'", names):
            env[k] = v
    for name, params, body in re.findall(r"const (\w+) = \(([^)]*)\) => (<.+?/>);", source):
        env["__helpers__"][name] = ([p.strip() for p in params.split(",")], body)
    m = re.search(r"return (<svg.*</svg>);", source, re.S)
    if not m:
        raise ValueError("no <svg> returned")
    svg = to_svg_attrs(substitute(m.group(1), env))

    light = ";".join(f"{k}:{v[0]}" for k, v in PALETTE.items())
    dark = ";".join(f"{k}:{v[1]}" for k, v in PALETTE.items())
    style = (
        "<style>svg{" + light + ";font-family:-apple-system,BlinkMacSystemFont,"
        "'Segoe UI',Helvetica,Arial,sans-serif}"
        "@media (prefers-color-scheme: dark){svg{" + dark + "}}</style>"
    )
    svg = svg.replace("<svg ", '<svg xmlns="http://www.w3.org/2000/svg" ', 1)
    svg = re.sub(r"(<svg[^>]*>)", r"\1" + style, svg, count=1)
    header = "<!-- Generated by scripts/render-architecture.py from docs/architecture/diagram.jsx. Do not edit. -->\n"
    return header + svg + "\n"


def main():
    rendered = render(SOURCE.read_text(encoding="utf-8"))
    if "--check" in sys.argv[1:]:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            print("docs/architecture.svg is stale: run scripts/render-architecture.py", file=sys.stderr)
            return 1
        return 0
    OUTPUT.write_text(rendered, encoding="utf-8")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
