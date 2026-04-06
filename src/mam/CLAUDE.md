# src/mam

Pure MyAnonamouse HTTP client layer. No MCP types, no rmcp macros, no schemars. Everything here is about talking to the MAM API and working with its data.

## Modules

| File | Purpose |
|---|---|
| `mod.rs` | HTTP client construction, `IpInfo`/`get_ip_info`, `enrich_error`, module re-exports |
| `types.rs` | Serde response structs: `SearchResponse`, `TorrentResult`, `TorrentDetail`, `UserDataResponse`, `BonusEntry` |
| `lookup.rs` | Static genre/language tables and pure mapping functions: `lookup_genres`, `map_languages`, `parse_sort`, `normalize_lookup` |
| `format.rs` | String formatters that turn response structs into human-readable text for the LLM |
| `api.rs` | Async free functions that make HTTP requests to MAM and return `Result<String, String>` |

## Dependency order

```
mod.rs  ←  api.rs  ←  types.rs
                   ←  format.rs  ←  types.rs
        ←  lookup.rs   (no internal deps)
```

`lookup.rs` and `format.rs` are pure (no I/O). `api.rs` is the only module that calls the network.

## Adding a new MAM API endpoint

1. Add a response struct to `types.rs` (if needed).
2. Add a free function to `api.rs` — signature: `pub(crate) async fn foo(client: &reqwest::Client, ...) -> Result<String, String>`.
3. Add a formatter to `format.rs` if the response needs rendering.
4. Wire it up as a thin wrapper tool in `src/tools/mod.rs`.

## Visibility

All items are `pub(crate)` — nothing here is part of the external binary API. The only public items are in `mod.rs`: `BASE_URL`, `build_client`, `IpInfo`, `get_ip_info`, `enrich_error` — used by `main.rs` for startup and `--test-connection`.
