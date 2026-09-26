// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;

use crate::persist::{self, StateStore};

// ---------------------------------------------------------------------------
// Token/code generation
// ---------------------------------------------------------------------------

/// Generate a 256-bit cryptographically random token, returned as base64url (no padding).
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    URL_SAFE_NO_PAD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Record types
// ---------------------------------------------------------------------------

pub struct ClientRecord {
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
    pub created_at: Instant,
    /// Set to `true` once a token has been issued for this client.
    pub authorized: bool,
}

pub struct AuthCode {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub expires_at: Instant,
}

pub struct AccessToken {
    pub client_id: String,
    pub expires_at: Instant,
}

pub struct RefreshToken {
    pub client_id: String,
    pub expires_at: Instant,
    /// Hard ceiling inherited along the rotation chain from the original
    /// authorization-code grant; sliding renewals never extend past it.
    pub family_expires_at: Instant,
    /// Set when this token is consumed by rotation: when, and the
    /// (access, refresh) pair that replaced it. Presenting it again within
    /// `REFRESH_GRACE_PERIOD` returns that same pair, so a retry after a lost
    /// response or two clients racing to refresh don't get bounced to re-auth.
    pub superseded: Option<(Instant, String, String)>,
}

pub struct PendingAuth {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: Option<String>,
    pub created_at: Instant,
}

// ---------------------------------------------------------------------------
// Caps and lifetimes
// ---------------------------------------------------------------------------

const MAX_CLIENTS: usize = 100;
const MAX_PENDING_AUTHS: usize = 1000;
pub const UNAUTHED_CLIENT_TTL: Duration = Duration::from_secs(15 * 60);
const AUTH_CODE_TTL: Duration = Duration::from_secs(10 * 60);
const ACCESS_TOKEN_TTL: Duration = Duration::from_secs(3600);
/// Sliding: every refresh mints a new token with a full TTL, so this is the
/// inactivity window before a client must re-authorize.
const REFRESH_TOKEN_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
/// Absolute lifetime of a refresh-token chain from the consent that started
/// it, so a stolen token cannot be kept alive indefinitely by refreshing.
pub const REFRESH_ABSOLUTE_TTL: Duration = Duration::from_secs(90 * 24 * 3600);
const PENDING_AUTH_TTL: Duration = Duration::from_secs(5 * 60);
pub const REFRESH_GRACE_PERIOD: Duration = Duration::from_secs(30);

// Re-export lifetimes needed by other modules
pub const ACCESS_TOKEN_LIFETIME_SECS: u64 = 3600;

// ---------------------------------------------------------------------------
// OAuthState
// ---------------------------------------------------------------------------

pub struct OAuthState {
    pub issuer: String,
    pub api_token: Option<String>,
    clients: Mutex<HashMap<String, ClientRecord>>,
    auth_codes: Mutex<HashMap<String, AuthCode>>,
    pending_auths: Mutex<HashMap<String, PendingAuth>>,
    access_tokens: Mutex<HashMap<String, AccessToken>>,
    refresh_tokens: Mutex<HashMap<String, RefreshToken>>,
    /// When set, mutations of long-lived maps mark the store dirty so the
    /// background flusher persists them. The store also seeds the maps at
    /// construction from whatever was loaded off disk.
    store: Option<Arc<StateStore>>,
}

impl OAuthState {
    /// Construct, seeding the long-lived maps (clients, access tokens, refresh
    /// tokens) from the persisted state store when one is provided.
    pub fn new(issuer: String, api_token: Option<String>, store: Option<Arc<StateStore>>) -> Self {
        let (clients, access_tokens, refresh_tokens) = match &store {
            Some(s) => s.oauth_runtime(),
            None => (HashMap::new(), HashMap::new(), HashMap::new()),
        };
        Self {
            issuer: issuer.trim_end_matches('/').to_string(),
            api_token,
            clients: Mutex::new(clients),
            auth_codes: Mutex::new(HashMap::new()),
            pending_auths: Mutex::new(HashMap::new()),
            access_tokens: Mutex::new(access_tokens),
            refresh_tokens: Mutex::new(refresh_tokens),
            store,
        }
    }

    fn mark_dirty(&self) {
        if let Some(store) = &self.store {
            store.mark_dirty();
        }
    }

    /// Take a snapshot of the long-lived maps in persisted form, under brief
    /// sync locks. The locks are released before this returns; no `.await` is
    /// held across them.
    pub(crate) fn snapshot(&self) -> persist::OAuthSection {
        let clients = self.clients.lock().unwrap();
        let access = self.access_tokens.lock().unwrap();
        let refresh = self.refresh_tokens.lock().unwrap();
        persist::OAuthSection::from_runtime(&clients, &access, &refresh)
    }

    // -- Clients --

    pub fn register_client(
        &self,
        redirect_uris: Vec<String>,
        client_name: Option<String>,
    ) -> Result<String, &'static str> {
        let mut clients = self.clients.lock().unwrap();
        if clients.len() >= MAX_CLIENTS {
            return Err("client registration limit reached");
        }
        let client_id = generate_token();
        clients.insert(
            client_id.clone(),
            ClientRecord {
                redirect_uris,
                client_name,
                created_at: Instant::now(),
                authorized: false,
            },
        );
        drop(clients);
        self.mark_dirty();
        Ok(client_id)
    }

    pub fn get_client(&self, client_id: &str) -> Option<(Vec<String>, Option<String>, bool)> {
        let clients = self.clients.lock().unwrap();
        clients
            .get(client_id)
            .map(|c| (c.redirect_uris.clone(), c.client_name.clone(), c.authorized))
    }

    pub fn mark_client_authorized(&self, client_id: &str) {
        let mut clients = self.clients.lock().unwrap();
        if let Some(c) = clients.get_mut(client_id) {
            if !c.authorized {
                c.authorized = true;
                drop(clients);
                self.mark_dirty();
            }
        }
    }

    pub fn client_exists(&self, client_id: &str) -> bool {
        self.clients.lock().unwrap().contains_key(client_id)
    }

    // -- Pending authorizations --

    pub fn insert_pending_auth(&self, nonce: String, pending: PendingAuth) -> Result<(), &'static str> {
        let mut auths = self.pending_auths.lock().unwrap();
        if auths.len() >= MAX_PENDING_AUTHS {
            return Err("too many pending authorizations");
        }
        auths.insert(nonce, pending);
        Ok(())
    }

    /// Remove and return the pending auth for the given nonce (single-use).
    pub fn take_pending_auth(&self, nonce: &str) -> Option<PendingAuth> {
        self.pending_auths.lock().unwrap().remove(nonce)
    }

    // -- Authorization codes --

    pub fn insert_auth_code(&self, client_id: String, redirect_uri: String, code_challenge: String) -> String {
        let code = generate_token();
        let mut codes = self.auth_codes.lock().unwrap();
        codes.insert(
            code.clone(),
            AuthCode {
                client_id,
                redirect_uri,
                code_challenge,
                expires_at: Instant::now() + AUTH_CODE_TTL,
            },
        );
        code
    }

    /// Remove and return the auth code (single-use). Returns None if expired or missing.
    pub fn take_auth_code(&self, code: &str) -> Option<AuthCode> {
        let mut codes = self.auth_codes.lock().unwrap();
        let ac = codes.remove(code)?;
        if Instant::now() > ac.expires_at {
            return None;
        }
        Some(ac)
    }

    // -- Access tokens --

    pub fn insert_access_token(&self, client_id: String) -> String {
        let token = generate_token();
        self.store_access_token(token.clone(), client_id);
        token
    }

    fn store_access_token(&self, token: String, client_id: String) {
        let mut tokens = self.access_tokens.lock().unwrap();
        tokens.insert(
            token,
            AccessToken {
                client_id,
                expires_at: Instant::now() + ACCESS_TOKEN_TTL,
            },
        );
        drop(tokens);
        self.mark_dirty();
    }

    /// Validate an access token. Returns the client_id if valid.
    pub fn validate_access_token(&self, token: &str) -> Option<String> {
        let tokens = self.access_tokens.lock().unwrap();
        let at = tokens.get(token)?;
        if Instant::now() > at.expires_at {
            return None;
        }
        Some(at.client_id.clone())
    }

    // -- Refresh tokens --

    pub fn insert_refresh_token(&self, client_id: String) -> String {
        let token = generate_token();
        let now = Instant::now();
        let mut tokens = self.refresh_tokens.lock().unwrap();
        tokens.insert(
            token.clone(),
            RefreshToken {
                client_id,
                expires_at: now + REFRESH_TOKEN_TTL,
                family_expires_at: now + REFRESH_ABSOLUTE_TTL,
                superseded: None,
            },
        );
        drop(tokens);
        self.mark_dirty();
        token
    }

    /// Consume a refresh token (rotation). Returns a new (access_token, refresh_token)
    /// pair, or None if the token is invalid/expired/past grace period/wrong client.
    /// Re-presenting a token within `REFRESH_GRACE_PERIOD` of its rotation
    /// returns the same pair that rotation produced (idempotent retry).
    pub fn rotate_refresh_token(&self, old_token: &str, client_id: &str) -> Option<(String, String)> {
        let now = Instant::now();
        let mut tokens = self.refresh_tokens.lock().unwrap();
        let rt = tokens.get_mut(old_token)?;

        // Verify the refresh token belongs to this client BEFORE we mark it superseded.
        if rt.client_id != client_id {
            return None;
        }

        let past_grace = matches!(&rt.superseded, Some((at, _, _)) if now > *at + REFRESH_GRACE_PERIOD);
        if now > rt.expires_at || past_grace {
            tokens.remove(old_token);
            drop(tokens);
            self.mark_dirty();
            return None;
        }

        if let Some((_, access, refresh)) = rt.superseded.clone() {
            // Lost-response retry: the successor is still unused, so hand
            // back the same answer.
            if tokens.get(&refresh).is_some_and(|s| s.superseded.is_none()) {
                return Some((access, refresh));
            }
            // The successor was already rotated by someone else, so this is a
            // replay of a captured token. Revoke everything the client holds:
            // the real client re-consents once, the holder of the copy is out.
            tokens.retain(|_, t| t.client_id != client_id);
            drop(tokens);
            self.access_tokens.lock().unwrap().retain(|_, t| t.client_id != client_id);
            self.mark_dirty();
            tracing::warn!(client_id, "refresh token reuse detected; revoked all tokens for client");
            return None;
        }

        // Mint the replacement pair under the same lock so a concurrent retry
        // sees it. The access token is generated here and stored just below.
        let new_access = generate_token();
        let new_refresh = generate_token();
        let family_expires_at = rt.family_expires_at;
        rt.superseded = Some((now, new_access.clone(), new_refresh.clone()));
        tokens.insert(
            new_refresh.clone(),
            RefreshToken {
                client_id: client_id.to_string(),
                expires_at: (now + REFRESH_TOKEN_TTL).min(family_expires_at),
                family_expires_at,
                superseded: None,
            },
        );
        drop(tokens);

        self.store_access_token(new_access.clone(), client_id.to_string());
        Some((new_access, new_refresh))
    }

    // -- Cleanup --

    /// Sweep all expired entries. Returns counts of swept items per category.
    pub fn sweep_expired(&self) -> SweepResult {
        let now = Instant::now();
        let mut result = SweepResult::default();

        // Expired auth codes
        {
            let mut codes = self.auth_codes.lock().unwrap();
            let before = codes.len();
            codes.retain(|_, c| now < c.expires_at);
            result.auth_codes = before - codes.len();
        }

        // Expired access tokens
        {
            let mut tokens = self.access_tokens.lock().unwrap();
            let before = tokens.len();
            tokens.retain(|_, t| now < t.expires_at);
            result.access_tokens = before - tokens.len();
        }

        // Expired or superseded-past-grace refresh tokens
        {
            let mut tokens = self.refresh_tokens.lock().unwrap();
            let before = tokens.len();
            tokens.retain(|_, t| {
                if now > t.expires_at {
                    return false;
                }
                !matches!(&t.superseded, Some((at, _, _)) if now > *at + REFRESH_GRACE_PERIOD)
            });
            result.refresh_tokens = before - tokens.len();
        }

        // Expired pending authorizations
        {
            let mut auths = self.pending_auths.lock().unwrap();
            let before = auths.len();
            auths.retain(|_, a| now < a.created_at + PENDING_AUTH_TTL);
            result.pending_auths = before - auths.len();
        }

        // Stale (unauthed) client registrations
        {
            let mut clients = self.clients.lock().unwrap();
            let before = clients.len();
            clients.retain(|_, c| c.authorized || now < c.created_at + UNAUTHED_CLIENT_TTL);
            result.stale_clients = before - clients.len();
        }

        if result.stale_clients > 0 || result.access_tokens > 0 || result.refresh_tokens > 0 {
            self.mark_dirty();
        }

        result
    }
}

#[derive(Default)]
pub struct SweepResult {
    pub auth_codes: usize,
    pub access_tokens: usize,
    pub refresh_tokens: usize,
    pub pending_auths: usize,
    pub stale_clients: usize,
}

impl SweepResult {
    pub fn has_any(&self) -> bool {
        self.auth_codes > 0
            || self.access_tokens > 0
            || self.refresh_tokens > 0
            || self.pending_auths > 0
            || self.stale_clients > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_retry_within_grace_returns_same_pair() {
        let state = OAuthState::new("http://localhost:8080".into(), None, None);
        let client_id = state.register_client(vec!["http://localhost/cb".into()], None).unwrap();
        let old = state.insert_refresh_token(client_id.clone());

        let first = state.rotate_refresh_token(&old, &client_id).expect("first rotation");
        let retry = state.rotate_refresh_token(&old, &client_id).expect("retry within grace");
        assert_eq!(first, retry);

        // Both tokens from the pair are live, and the wrong client is still rejected.
        assert_eq!(state.validate_access_token(&first.0), Some(client_id.clone()));
        assert!(state.rotate_refresh_token(&first.1, &client_id).is_some());
        assert!(state.rotate_refresh_token(&old, "someone-else").is_none());
    }

    #[test]
    fn refresh_replay_after_successor_rotated_revokes_client() {
        let state = OAuthState::new("http://localhost:8080".into(), None, None);
        let client_id = state.register_client(vec!["http://localhost/cb".into()], None).unwrap();
        let r1 = state.insert_refresh_token(client_id.clone());
        let (a2, r2) = state.rotate_refresh_token(&r1, &client_id).unwrap();
        let (a3, r3) = state.rotate_refresh_token(&r2, &client_id).unwrap();

        // r1 is two generations old: a replay, not a retry. Everything dies.
        assert!(state.rotate_refresh_token(&r1, &client_id).is_none());
        assert!(state.validate_access_token(&a2).is_none());
        assert!(state.validate_access_token(&a3).is_none());
        assert!(state.rotate_refresh_token(&r3, &client_id).is_none());
    }

    #[test]
    fn refresh_rotation_never_extends_past_family_cap() {
        let state = OAuthState::new("http://localhost:8080".into(), None, None);
        let cap = Instant::now() + Duration::from_secs(60);
        state.refresh_tokens.lock().unwrap().insert(
            "r1".into(),
            RefreshToken { client_id: "c".into(), expires_at: cap, family_expires_at: cap, superseded: None },
        );
        let (_, r2) = state.rotate_refresh_token("r1", "c").unwrap();
        let tokens = state.refresh_tokens.lock().unwrap();
        assert_eq!(tokens[&r2].expires_at, cap);
        assert_eq!(tokens[&r2].family_expires_at, cap);
    }
}
