# src/tools

MCP layer only. This module owns the tool definitions visible to the LLM — parameter schemas, tool metadata, the server handler. All MAM API logic lives in `src/mam/`.

## What lives here

| Item | Purpose |
|---|---|
| `MamServer` | Holds `Arc<reqwest::Client>`, `ToolRouter<Self>`, and `HashSet<String>` of enabled tool names |
| `MamServer::new()` | Builds the router and removes disabled tools via `remove_route()` |
| `TOOL_REGISTRY` | Source of truth for tool names, groups (`default`/`power`/`user`/`seedbox`), and default-on status — read by `main.rs` for `--list-tools` and startup enable logic |
| `ALL_TOOL_NAMES` | Flat list of every tool name; used to iterate for `remove_route()` |
| Parameter structs | One per tool — derive `Deserialize` + `schemars::JsonSchema`; field doc comments become the JSON Schema descriptions the LLM sees |
| `#[tool_router]` impl | Thin async wrappers: parse params → call `crate::mam::api::*` or `crate::mam::lookup::*` → return result |
| `ServerHandler` impl | `get_info()` builds server capabilities and enabled-tool instructions; `set_level()` is a no-op stub |

## Tool method pattern

```rust
async fn search_audiobooks(&self, Parameters(p): Parameters<SearchAudiobooksParams>) -> Result<String, String> {
    let cat = crate::mam::lookup::lookup_genres(p.genre.as_deref().unwrap_or(&[]), ...)?;
    let lang = crate::mam::lookup::map_languages(p.language.as_deref().unwrap_or(&[]))?;
    let sort = crate::mam::lookup::parse_sort(p.sort.as_deref().unwrap_or(""))?;
    crate::mam::api::do_search(&self.client, &p.query, vec![13], cat, lang, sort, ...).await
}
```

Input validation that belongs to the MCP interface (e.g. "provide `id` or `hash`, not neither") stays in the tool method. Everything else — HTTP calls, response formatting, lookup table resolution — goes in `src/mam/`.

## Adding a new tool

1. Add a parameter struct deriving `Deserialize + schemars::JsonSchema`.
2. Add an async method to the `#[tool_router]` impl with a `#[tool(...)]` attribute.
3. Add the tool to `TOOL_REGISTRY` and `ALL_TOOL_NAMES`.
4. Implement the API call and formatter in `src/mam/api.rs` and `src/mam/format.rs`.

## serde helpers

- `remove_null_default` — schemars transform to strip `"default": null` from optional fields that use `#[serde(default)]`
- `string_or_vec` — custom deserializer accepting either `"string"` or `["array"]` for `Option<Vec<String>>` fields; LLMs sometimes pass a bare string
