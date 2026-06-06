// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

//! File-based persistence for long-lived server state: the current (possibly
//! rotated) `mam_id` session cookie, plus — when OAuth is enabled — client
//! registrations, access tokens, and refresh tokens. Short-lived OAuth state
//! (auth codes, pending consent sessions) is not persisted — those flows
//! simply restart after a server restart.
//!
//! Writes are debounced: mutations set a dirty flag on `StateStore`, and a
//! background task flushes every `FLUSH_INTERVAL` if dirty. File writes are
//! atomic via `tmp` + `rename`. On Unix, the file is chmod'd to 0600 before
//! rename because it contains bearer tokens and the session cookie.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::mam::SessionJar;
use crate::oauth::state::{
    AccessToken, ClientRecord, OAuthState, REFRESH_GRACE_PERIOD, RefreshToken, UNAUTHED_CLIENT_TTL,
};

/// Schema version for the on-disk format. Bump when the JSON shape changes.
/// (`mam_session` was added as an optional field without a bump — old files
/// parse fine, and old binaries ignore the unknown field.)
pub const SCHEMA_VERSION: u32 = 1;

/// How often the background task wakes to flush dirty state.
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// On-disk schema
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mam_session: Option<PersistedMamSession>,
    #[serde(flatten)]
    pub oauth: OAuthSection,
}

/// Tracks the `mam_id` session cookie across restarts. `seed` is the value the
/// user originally supplied (via `--mam-session` / `MAM_SESSION`); `current`
/// is the latest value after any `Set-Cookie` rotations by MAM.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct PersistedMamSession {
    pub seed: String,
    pub current: String,
    pub updated_at_unix: u64,
}

/// The long-lived OAuth maps. Flattened into `PersistedState` so the on-disk
/// shape is unchanged from when this file only held OAuth state.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct OAuthSection {
    #[serde(default)]
    pub clients: HashMap<String, PersistedClient>,
    #[serde(default)]
    pub access_tokens: HashMap<String, PersistedToken>,
    #[serde(default)]
    pub refresh_tokens: HashMap<String, PersistedRefresh>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedClient {
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub client_name: Option<String>,
    pub created_at_unix: u64,
    pub authorized: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedToken {
    pub client_id: String,
    pub expires_at_unix: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedRefresh {
    pub client_id: String,
    pub expires_at_unix: u64,
    #[serde(default)]
    pub superseded_at_unix: Option<u64>,
}

impl PersistedState {
    pub fn empty() -> Self {
        Self {
            version: SCHEMA_VERSION,
            mam_session: None,
            oauth: OAuthSection::default(),
        }
    }
}

impl OAuthSection {
    /// Build an `OAuthSection` from current runtime maps.
    pub fn from_runtime(
        clients: &HashMap<String, ClientRecord>,
        access_tokens: &HashMap<String, AccessToken>,
        refresh_tokens: &HashMap<String, RefreshToken>,
    ) -> Self {
        let now_i = Instant::now();
        let now_s = SystemTime::now();

        let clients = clients
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PersistedClient {
                        redirect_uris: v.redirect_uris.clone(),
                        client_name: v.client_name.clone(),
                        created_at_unix: instant_to_unix(v.created_at, now_i, now_s),
                        authorized: v.authorized,
                    },
                )
            })
            .collect();

        let access_tokens = access_tokens
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PersistedToken {
                        client_id: v.client_id.clone(),
                        expires_at_unix: instant_to_unix(v.expires_at, now_i, now_s),
                    },
                )
            })
            .collect();

        let refresh_tokens = refresh_tokens
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PersistedRefresh {
                        client_id: v.client_id.clone(),
                        expires_at_unix: instant_to_unix(v.expires_at, now_i, now_s),
                        superseded_at_unix: v
                            .superseded_at
                            .map(|i| instant_to_unix(i, now_i, now_s)),
                    },
                )
            })
            .collect();

        Self {
            clients,
            access_tokens,
            refresh_tokens,
        }
    }

    /// Hydrate runtime maps from an `OAuthSection`. Entries that have already
    /// expired at load time are dropped — they'd be swept seconds later anyway.
    pub fn into_runtime(
        self,
    ) -> (
        HashMap<String, ClientRecord>,
        HashMap<String, AccessToken>,
        HashMap<String, RefreshToken>,
    ) {
        let now_i = Instant::now();
        let now_s = SystemTime::now();

        let mut clients = HashMap::new();
        for (id, pc) in self.clients {
            // Authorized clients never get swept by age, so created_at is irrelevant.
            // For unauthorized clients, drop any that have already exceeded the TTL.
            let created_at = if pc.authorized {
                now_i
            } else {
                let saved = UNIX_EPOCH + Duration::from_secs(pc.created_at_unix);
                let age = now_s.duration_since(saved).unwrap_or(Duration::ZERO);
                if age >= UNAUTHED_CLIENT_TTL {
                    continue;
                }
                now_i.checked_sub(age).unwrap_or(now_i)
            };
            clients.insert(
                id,
                ClientRecord {
                    redirect_uris: pc.redirect_uris,
                    client_name: pc.client_name,
                    created_at,
                    authorized: pc.authorized,
                },
            );
        }

        let mut access_tokens = HashMap::new();
        for (tok, pt) in self.access_tokens {
            if let Some(expires_at) = unix_to_instant(pt.expires_at_unix, now_i, now_s) {
                if expires_at > now_i {
                    access_tokens.insert(
                        tok,
                        AccessToken {
                            client_id: pt.client_id,
                            expires_at,
                        },
                    );
                }
            }
        }

        let mut refresh_tokens = HashMap::new();
        for (tok, pr) in self.refresh_tokens {
            let Some(expires_at) = unix_to_instant(pr.expires_at_unix, now_i, now_s) else {
                continue;
            };
            if expires_at <= now_i {
                continue;
            }

            // For superseded refresh tokens, the grace window is only 30 s —
            // if restart took longer than that, drop the token outright.
            let superseded_at = match pr.superseded_at_unix {
                None => None,
                Some(ts) => {
                    let saved = UNIX_EPOCH + Duration::from_secs(ts);
                    let since = now_s.duration_since(saved).unwrap_or(Duration::ZERO);
                    if since >= REFRESH_GRACE_PERIOD {
                        continue;
                    }
                    Some(now_i.checked_sub(since).unwrap_or(now_i))
                }
            };

            refresh_tokens.insert(
                tok,
                RefreshToken {
                    client_id: pr.client_id,
                    expires_at,
                    superseded_at,
                },
            );
        }

        (clients, access_tokens, refresh_tokens)
    }
}

// ---------------------------------------------------------------------------
// StateStore
// ---------------------------------------------------------------------------

/// Owns the state file: path, debounced dirty flag, the live `mam_id` session
/// record, and the OAuth section as loaded from disk (used as the write-back
/// fallback when no live `OAuthState` is attached).
pub struct StateStore {
    path: PathBuf,
    dirty: AtomicBool,
    mam_session: Mutex<Option<PersistedMamSession>>,
    /// OAuth section as loaded from disk — written back verbatim when no live
    /// `OAuthState` is attached (stdio transport, plain HTTP), so a non-OAuth
    /// run never clobbers persisted OAuth state.
    loaded_oauth: Mutex<OAuthSection>,
}

impl StateStore {
    /// Load the state file. A missing file is treated as empty — no error.
    /// A corrupt or unknown-version file is renamed aside and treated as empty.
    pub async fn load(path: PathBuf) -> anyhow::Result<Arc<Self>> {
        let loaded = load_file(&path).await?;
        info!(
            path = %path.display(),
            clients = loaded.oauth.clients.len(),
            access_tokens = loaded.oauth.access_tokens.len(),
            refresh_tokens = loaded.oauth.refresh_tokens.len(),
            has_mam_session = loaded.mam_session.is_some(),
            "Loaded persisted state",
        );
        Ok(Arc::new(Self {
            path,
            dirty: AtomicBool::new(false),
            mam_session: Mutex::new(loaded.mam_session),
            loaded_oauth: Mutex::new(loaded.oauth),
        }))
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Resolve the effective `mam_id` for this run. If the stored record's seed
    /// matches `provided`, the stored (possibly rotated) value wins. If
    /// `provided` differs from the stored seed, the user supplied a new cookie —
    /// start a fresh record and discard the stored one.
    pub fn resolve_session(&self, provided: &str) -> String {
        let mut rec = self.mam_session.lock().unwrap();
        match rec.as_ref() {
            Some(r) if r.seed == provided => {
                if r.current != r.seed {
                    info!("Using rotated mam_id from state file");
                }
                r.current.clone()
            }
            prior => {
                if prior.is_some() {
                    info!("Provided mam_id differs from stored seed; starting a fresh session record");
                }
                *rec = Some(PersistedMamSession {
                    seed: provided.to_string(),
                    current: provided.to_string(),
                    updated_at_unix: now_unix(),
                });
                drop(rec);
                self.mark_dirty();
                provided.to_string()
            }
        }
    }

    /// Record the latest `mam_id` observed from the session jar. No-op if unchanged.
    pub fn update_session(&self, current: &str) {
        let mut rec = self.mam_session.lock().unwrap();
        match rec.as_mut() {
            Some(r) if r.current == current => return,
            Some(r) => {
                r.current = current.to_string();
                r.updated_at_unix = now_unix();
            }
            None => {
                *rec = Some(PersistedMamSession {
                    seed: current.to_string(),
                    current: current.to_string(),
                    updated_at_unix: now_unix(),
                });
            }
        }
        drop(rec);
        self.mark_dirty();
        info!("Tracking rotated mam_id session cookie");
    }

    /// Force an immediate flush to disk, regardless of the dirty flag.
    /// Used on graceful shutdown to capture any mutations from the last flush interval.
    pub async fn flush(&self, oauth: Option<&OAuthState>) -> anyhow::Result<()> {
        self.dirty.store(false, Ordering::Relaxed);
        let snapshot = self.snapshot(oauth);
        save(&self.path, &snapshot).await
    }

    /// Flush only if the dirty flag is set. Clears the flag *before* writing so
    /// concurrent mutations during the write re-mark it for the next tick.
    pub async fn flush_if_dirty(&self, oauth: Option<&OAuthState>) -> anyhow::Result<()> {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        let snapshot = self.snapshot(oauth);
        if let Err(e) = save(&self.path, &snapshot).await {
            // Re-set dirty so the next tick retries.
            self.dirty.store(true, Ordering::Relaxed);
            return Err(e);
        }
        Ok(())
    }

    /// Take a snapshot of persisted state under brief sync locks. The locks are
    /// released before this returns; no `.await` is held across them.
    fn snapshot(&self, oauth: Option<&OAuthState>) -> PersistedState {
        let oauth_section = match oauth {
            Some(state) => state.snapshot(),
            None => self.loaded_oauth.lock().unwrap().clone(),
        };
        PersistedState {
            version: SCHEMA_VERSION,
            mam_session: self.mam_session.lock().unwrap().clone(),
            oauth: oauth_section,
        }
    }

    /// Convert the OAuth section loaded from disk into runtime maps for
    /// seeding a fresh `OAuthState`.
    pub fn oauth_runtime(
        &self,
    ) -> (
        HashMap<String, ClientRecord>,
        HashMap<String, AccessToken>,
        HashMap<String, RefreshToken>,
    ) {
        self.loaded_oauth.lock().unwrap().clone().into_runtime()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Instant ↔ Unix helpers
// ---------------------------------------------------------------------------

fn instant_to_unix(i: Instant, now_i: Instant, now_s: SystemTime) -> u64 {
    let sys = if i >= now_i {
        now_s + (i - now_i)
    } else {
        now_s.checked_sub(now_i - i).unwrap_or(UNIX_EPOCH)
    };
    sys.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_to_instant(ts: u64, now_i: Instant, now_s: SystemTime) -> Option<Instant> {
    let target = UNIX_EPOCH + Duration::from_secs(ts);
    if target >= now_s {
        Some(now_i + target.duration_since(now_s).ok()?)
    } else {
        let delta = now_s.duration_since(target).ok()?;
        now_i.checked_sub(delta)
    }
}

// ---------------------------------------------------------------------------
// Disk I/O
// ---------------------------------------------------------------------------

async fn load_file(path: &Path) -> anyhow::Result<PersistedState> {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PersistedState::empty()),
        Err(e) => return Err(e.into()),
    };

    match serde_json::from_slice::<PersistedState>(&bytes) {
        Ok(state) if state.version == SCHEMA_VERSION => Ok(state),
        Ok(state) => {
            warn!(
                path = %path.display(),
                version = state.version,
                expected = SCHEMA_VERSION,
                "Unknown state schema version; starting with empty state",
            );
            rename_corrupt(path).await;
            Ok(PersistedState::empty())
        }
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to parse state file; starting with empty state",
            );
            rename_corrupt(path).await;
            Ok(PersistedState::empty())
        }
    }
}

async fn save(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
    let json = serde_json::to_vec_pretty(state)?;
    let tmp_path = tmp_sibling(path, ".tmp");

    tokio::fs::write(&tmp_path, &json).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600)).await?;
    }

    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

async fn rename_corrupt(path: &Path) {
    let ts = now_unix();
    let corrupt = tmp_sibling(path, &format!(".corrupt-{ts}"));
    match tokio::fs::rename(path, &corrupt).await {
        Ok(()) => warn!(from = %path.display(), to = %corrupt.display(), "Renamed corrupt state file"),
        Err(e) => warn!(from = %path.display(), to = %corrupt.display(), error = %e, "Failed to rename corrupt state file"),
    }
}

fn tmp_sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

// ---------------------------------------------------------------------------
// Background flusher
// ---------------------------------------------------------------------------

/// Spawn the debounced flusher. Each tick it picks up any rotated `mam_id`
/// from the session jar, then flushes to disk if anything is dirty.
pub fn spawn_flusher(
    store: Arc<StateStore>,
    jar: Arc<SessionJar>,
    oauth: Option<Arc<OAuthState>>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(FLUSH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            store.update_session(&jar.current());
            if let Err(e) = store.flush_if_dirty(oauth.as_deref()).await {
                warn!(error = %e, "State flush failed");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::state::OAuthState;

    fn temp_state_path(tag: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("mam-mcp-state-{tag}-{ts}.json"))
    }

    #[test]
    fn instant_roundtrip_future() {
        let now_i = Instant::now();
        let now_s = SystemTime::now();
        let future = now_i + Duration::from_secs(300);
        let ts = instant_to_unix(future, now_i, now_s);
        let back = unix_to_instant(ts, now_i, now_s).unwrap();
        let diff = back.duration_since(future);
        assert!(diff < Duration::from_secs(1), "diff too large: {diff:?}");
    }

    #[test]
    fn instant_roundtrip_past_within_uptime() {
        let now_i = Instant::now();
        let now_s = SystemTime::now();
        if let Some(past) = now_i.checked_sub(Duration::from_millis(100)) {
            let ts = instant_to_unix(past, now_i, now_s);
            let back = unix_to_instant(ts, now_i, now_s).unwrap();
            let diff = now_i.duration_since(back);
            assert!(diff < Duration::from_secs(1), "diff too large: {diff:?}");
        }
    }

    #[test]
    fn empty_persisted_state_roundtrips() {
        let empty = PersistedState::empty();
        let bytes = serde_json::to_vec(&empty).unwrap();
        let back: PersistedState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.version, SCHEMA_VERSION);
        assert!(back.oauth.clients.is_empty());
        assert!(back.mam_session.is_none());
    }

    #[test]
    fn legacy_oauth_only_file_parses() {
        // A file written before mam_session existed — top-level OAuth maps, no
        // mam_session key — must still parse.
        let json = r#"{"version":1,"clients":{},"access_tokens":{},"refresh_tokens":{}}"#;
        let back: PersistedState = serde_json::from_str(json).unwrap();
        assert_eq!(back.version, SCHEMA_VERSION);
        assert!(back.mam_session.is_none());
    }

    #[tokio::test]
    async fn oauth_state_save_load_roundtrip() {
        let path = temp_state_path("roundtrip");
        let _ = tokio::fs::remove_file(&path).await;

        // Seed first instance.
        let (client_id, access, refresh) = {
            let store = StateStore::load(path.clone()).await.unwrap();
            let state = OAuthState::new(
                "http://localhost:8080".into(),
                None,
                Some(store.clone()),
            );

            let client_id = state
                .register_client(
                    vec!["http://localhost:9000/cb".into()],
                    Some("test client".into()),
                )
                .unwrap();
            state.mark_client_authorized(&client_id);
            let access = state.insert_access_token(client_id.clone());
            let refresh = state.insert_refresh_token(client_id.clone());

            store.flush(Some(&state)).await.unwrap();
            (client_id, access, refresh)
        };

        // Load a fresh instance from the same file.
        let store = StateStore::load(path.clone()).await.unwrap();
        let reloaded = OAuthState::new(
            "http://localhost:8080".into(),
            None,
            Some(store),
        );

        assert!(reloaded.client_exists(&client_id));
        let info = reloaded.get_client(&client_id).expect("client present");
        assert_eq!(info.1, Some("test client".into()));
        assert!(info.2, "client should still be marked authorized");

        // Access token survives reload and validates to the original client.
        assert_eq!(
            reloaded.validate_access_token(&access),
            Some(client_id.clone())
        );

        // Refresh token survives reload and rotates successfully.
        let rotated = reloaded.rotate_refresh_token(&refresh, &client_id);
        assert!(rotated.is_some(), "refresh token should be usable after reload");

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn missing_file_loads_as_empty() {
        let path = temp_state_path("missing");
        let _ = tokio::fs::remove_file(&path).await;

        let store = StateStore::load(path.clone()).await.unwrap();
        let state = OAuthState::new("http://localhost:8080".into(), None, Some(store));

        assert!(!state.client_exists("anything"));
    }

    #[tokio::test]
    async fn corrupt_file_is_renamed_and_recovered() {
        let path = temp_state_path("corrupt");
        tokio::fs::write(&path, b"{not valid json").await.unwrap();

        let store = StateStore::load(path.clone()).await.unwrap();
        let state = OAuthState::new("http://localhost:8080".into(), None, Some(store));

        // Starts with empty state (no crash).
        assert!(!state.client_exists("anything"));

        // Original corrupt file has been moved aside.
        assert!(!path.exists(), "corrupt file should be renamed away");

        // Clean up any .corrupt-* siblings.
        if let Some(parent) = path.parent() {
            if let Some(stem) = path.file_name() {
                let prefix = format!("{}.corrupt-", stem.to_string_lossy());
                if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name();
                        if name.to_string_lossy().starts_with(&prefix) {
                            let _ = tokio::fs::remove_file(entry.path()).await;
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn mam_session_rotation_survives_restart() {
        let path = temp_state_path("session-rotate");
        let _ = tokio::fs::remove_file(&path).await;

        // First run: seed from env, observe a rotation, flush.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("seed-cookie"), "seed-cookie");
            store.update_session("rotated-cookie");
            store.flush(None).await.unwrap();
        }

        // Second run with the same env value: rotated value wins.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("seed-cookie"), "rotated-cookie");
        }

        // Third run with a NEW env value: the new cookie wins, record resets.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("new-cookie"), "new-cookie");
            store.flush(None).await.unwrap();
        }

        // Fourth run: the new seed is now stored.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("new-cookie"), "new-cookie");
        }

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn non_oauth_flush_preserves_oauth_section() {
        let path = temp_state_path("preserve-oauth");
        let _ = tokio::fs::remove_file(&path).await;

        // Run 1: HTTP+OAuth run persists a client.
        let client_id = {
            let store = StateStore::load(path.clone()).await.unwrap();
            let state = OAuthState::new(
                "http://localhost:8080".into(),
                None,
                Some(store.clone()),
            );
            let client_id = state
                .register_client(vec!["http://localhost:9000/cb".into()], None)
                .unwrap();
            state.mark_client_authorized(&client_id);
            store.flush(Some(&state)).await.unwrap();
            client_id
        };

        // Run 2: stdio run (no OAuthState) records a session rotation and flushes.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("seed"), "seed");
            store.update_session("rotated");
            store.flush(None).await.unwrap();
        }

        // Run 3: OAuth run again — the client registration must have survived,
        // and the rotated session must still be there.
        {
            let store = StateStore::load(path.clone()).await.unwrap();
            assert_eq!(store.resolve_session("seed"), "rotated");
            let state = OAuthState::new("http://localhost:8080".into(), None, Some(store));
            assert!(state.client_exists(&client_id), "OAuth client must survive a non-OAuth flush");
        }

        let _ = tokio::fs::remove_file(&path).await;
    }
}
