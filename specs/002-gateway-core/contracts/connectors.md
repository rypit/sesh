# Contract: connectors

| Method | Input | Output |
|---|---|---|
| `search.web` | `{query: string(1..300), limit?: integer(1..20)=5}` | `[{title, url, snippet}]` |
| `search.wiki` | `{query: string(1..300), limit?: integer(1..20)=5}` | `[{title, path, snippet}]` |
| `search.article` | `{path: string(1..500)}` | `{title, text}` (text capped) |
| `geo.forecast` | `{latitude?: number(-90..90), longitude?: number(-180..180), days?: integer(1..7)=3}` | `{latitude, longitude, days: [{date, max_c, min_c, precipitation_mm, summary}]}` |
| `geo.geocode` | `{query: string(1..200)}` | `[{name, latitude, longitude}]` (max 5) |
| `geo.nearby` | `{kind: string(1..50), radius_km?: number(0.1..20)=2}` | `[{name, latitude, longitude}]` (max 10) |

No method has side effects; none requires approval. A test-only
connector `testgate.act` (enabled by `SESH_GATEWAY_TEST_CONNECTORS=1`)
requires approval, for FR-008's negative test.
