# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

This is an MCP (Model Context Protocol) server for interacting with the MyAnonamouse private torrent tracker. The project is written in Rust.

## Commands

```bash
# Build the project
cargo build
# Run the project
cargo run
# Run tests
cargo test
# Build documentation
cargo doc --open
```

## Tech Stack

Rust for the code. Cargo for package management and build system.

## Dependencies

| Crate | Purpose |
|---|---|
| `rmcp` | Official MCP Rust SDK — features: `server`, `transport-io`, `transport-streamable-http-server`, `schemars` |
| `tokio` | Async runtime — features: `full` |
| `reqwest` | HTTP client for MAM API calls — features: `json`, `cookies` |
| `serde` | Serialization — features: `derive` |
| `serde_json` | JSON support |
| `anyhow` | Error propagation |
| `thiserror` | Custom error types |
| `axum` | HTTP server for HTTP/SSE transport |
| `tower-http` | Middleware — features: `cors`, `trace` |
| `tracing` | Logging |
| `tracing-subscriber` | Log output formatting — **must be configured to write to stderr or a file, never stdout**. Any output on stdout corrupts the JSON-RPC framing used by the MCP stdio transport. — features: `env-filter` |
| `clap` | CLI args — features: `derive`, `env` |
| `chrono` | Timestamp formatting — features: `std`, no default features |

## Authentication

MyAnonamouse uses a **session cookie** named `mam_id`. There is no login endpoint or token exchange — the user must obtain the cookie value from the Security tab of their Preferences on MyAnonamouse and provide it to the server.

- **Supply via:** `--mam-session <value>` CLI arg or `MAM_SESSION` environment variable
- **How to obtain:** Log into MyAnonamouse, go to Preferences → Security tab, copy the `mam_id` value
- **Transmission:** Injected as a `Cookie: mam_id=<value>` header on every outbound HTTP request
- **Headers set on every request:**
  - `Cookie: mam_id=<value>`
  - `Content-Type: application/json`
  - `User-Agent: Mozilla/5.0` (browser spoof required by MAM)
- **No session management:** The cookie is assumed to be valid; there is no refresh or re-auth logic
- **Base URL:** `https://www.myanonamouse.net`

## Architecture

```
MCP Client (Claude Desktop, etc.)
        ↕ MCP (stdio or HTTP/SSE)
  MamServer (src/tools/mod.rs)
        ↕ shared HTTP client
  MAM HTTP API (www.myanonamouse.net)
```

A single HTTP client is built once at startup with the `mam_id` cookie and User-Agent pre-configured, then shared across all tool calls. The `MamServer` struct holds this shared client plus the set of enabled tool names, and is cloned per HTTP session when using the HTTP transport.

## File Structure

| Path | Purpose |
|---|---|
| `Cargo.toml` | Manifest — dependencies, metadata, binary definition |
| `Cargo.lock` | Exact dependency versions (kept for binaries) |
| `src/main.rs` | Entry point — CLI arg parsing, tool-enable/disable flag processing, transport selection, server startup, HTTP auth middleware |
| `src/mam/mod.rs` | MAM HTTP client — builds the shared client, `get_ip_info` helper, `enrich_error` with LLM hints |
| `src/tools/mod.rs` | `MamServer` struct + all MCP tool implementations + genre/language/sort lookup tables + server handler |
| `tests/` | Integration tests |
| `api-docs/` | MAM API documentation (HTML); `Search-Form-HTML-fragment.html` is the primary source for search parameter names and category/language IDs |

## Tool Implementation Pattern

Tools are defined as async methods on `MamServer` using three rmcp procedural macros: `#[tool_router]` on the impl block, `#[tool]` on each tool method, and `#[tool_handler]` on the `ServerHandler` impl.

- Each tool method's doc comment becomes the tool description visible to the LLM — write it as a complete sentence
- Tool methods always return `Result<String, String>` — the Ok value is the result text sent to the LLM, the Err value is the error text
- Parameters are defined as structs deriving `Deserialize` and `schemars::JsonSchema` — each field's doc comment becomes its JSON Schema description visible to the LLM
- Optional parameters use `Option<T>`; boolean parameters that default to false use `#[serde(default)]`

## Tools

Tools are grouped by opt-in flag. The default set is lean to minimise LLM token usage.

| Tool | Group | Default | Description |
|---|---|---|---|
| `search_audiobooks` | default | ✓ | Search audiobooks by query, genre name, language name |
| `search_ebooks` | default | ✓ | Search ebooks by query, genre name, language name |
| `search_music` | default | ✓ | Search musicology content by query, genre name |
| `search_radio` | default | ✓ | Search radio content by query, genre name |
| `get_torrent_details` | default | ✓ | Full details for one torrent by ID or hash |
| `get_top_10` | default | ✓ | Top 10 most-snatched torrents by period/category |
| `get_ip_info` | seedbox | — | Current IP and ASN as seen by MAM |
| `search_torrents` | power | — | Cross-category power search with raw category/language IDs |
| `list_categories` | power | — | Returns full category/subcategory ID table for `search_torrents` |
| `get_user_data` | user | — | User profile — stats, ratio, notifications |
| `get_user_bonus_history` | user | — | Bonus point transaction history |
| `update_seedbox_ip` | seedbox | — | Register current IP as dynamic seedbox IP |

### Tool design

- Friendly search tools accept genre and language as plain strings; the server maps them to numeric IDs internally via fuzzy lookup tables in `src/tools/mod.rs`.
- Sort order is accepted as natural language ("newest", "most seeders", "title a-z") or raw API strings ("dateDesc", "seedersDesc") via `parse_sort()`.
- All search tools support `limit` (default 20, max 100) and `offset` for pagination.
- `TOOL_REGISTRY` in `src/tools/mod.rs` is the single source of truth for tool names, groups, and defaults. `--list-tools` reads it directly.
- Disabled tools are removed from the `ToolRouter` at startup via `remove_route()` so they are invisible to the LLM and consume no token budget.

## CLI Args

The server accepts these flags:

- `--mam-session` / `MAM_SESSION` env — the `mam_id` session cookie (required unless `--list-tools`)
- `--transport` — `stdio` (default) or `http`
- `--http-bind` — bind address for HTTP transport (default `0.0.0.0:8080`)
- `--api-token` / `MAM_API_TOKEN` env — Bearer token for HTTP transport authentication
- `--oauth-issuer` / `MAM_OAUTH_ISSUER` env — base URL of this server as the OAuth 2.1 issuer; enables the embedded OAuth authorization server for HTTP transport
- `--oauth-state-file` / `MAM_OAUTH_STATE_FILE` env — path to a JSON file used to persist OAuth client registrations and access/refresh tokens across restarts
- `--enable-power-tools` — enable `search_torrents` + `list_categories`
- `--enable-user-tools` — enable `get_user_data` + `get_user_bonus_history`
- `--enable-seedbox` — enable `update_seedbox_ip`
- `--enable-tool=<name>` — enable a specific tool by name (repeatable)
- `--disable-tool=<name>` — disable a specific tool (repeatable, always wins)
- `--list-tools` — print all tools with group and default status, then exit
- `--test-connection` — verify the session cookie works, then exit

## Error Handling

- `anyhow` is used for all internal error propagation
- `thiserror` is used for any custom error enums
- Tool methods convert internal errors to plain strings at the boundary
- Known HTTP error codes get `[Hint: ...]` suffixes appended to their error messages so the LLM knows how to recover — for example, a 401 tells the user to refresh their `mam_id` from Preferences → Security, and a 429 tells the LLM to wait before retrying

## Logging

All log output goes to **stderr** via `tracing` — never stdout, which is reserved for MCP JSON-RPC framing. Logs use structured key-value fields rather than inline format strings. Log level defaults to INFO and respects the `RUST_LOG` environment variable. Never use `println!`.

## Transport

**Stdio** (default): the server speaks MCP over stdin/stdout. This is the transport used by Claude Desktop.

**HTTP**: the server listens on the configured bind address and exposes the MCP endpoint at `/mcp`. Each connection gets its own `MamServer` instance. Requests are authenticated via a Bearer token if `--api-token` is set. The server applies CORS and HTTP tracing middleware.

### OAuth State Persistence

By default OAuth state is held in memory only — every registered client, access token, and refresh token is lost on restart, so every MCP client has to re-register and re-consent. Passing `--oauth-state-file <PATH>` (or `MAM_OAUTH_STATE_FILE=<PATH>`) activates file-backed persistence for long-lived state.

**What is persisted**: `clients` (dynamic client registrations), `access_tokens`, `refresh_tokens`.
**What is not persisted**: authorization codes (10-min TTL) and pending consent-page sessions (5-min TTL). An in-flight OAuth flow interrupted by a restart simply restarts from the beginning.

**Write strategy**: mutations set a dirty flag; a background task flushes every ~2 s if dirty. A final flush runs on graceful shutdown (Ctrl-C) to capture the last-interval window. Writes are atomic: JSON is written to `<path>.tmp` and then renamed over `<path>`.

**File permissions**: on Unix the file is chmod'd to `0600` because it contains bearer tokens. On Windows the file inherits default ACLs — choose a path inside a user-only directory.

**Missing file on startup**: treated as empty. First-run behaviour is unchanged.

**Corrupt or unknown-version file**: logged as a `WARN`, renamed to `<path>.corrupt-<unix_ts>` for recovery, and startup proceeds with empty state rather than crashing. Clients re-register; no data beyond OAuth sessions is affected.
