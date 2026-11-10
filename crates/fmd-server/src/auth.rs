use crate::AppState;
use axum::{
    extract::{FromRef, FromRequestParts, Path, State},
    http::{
        header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, SET_COOKIE},
        request::Parts,
        StatusCode,
    },
    Json,
};
use chrono::Utc;
use fmd_core::{
    auth::{self, Role, User, UserPermissions},
    Database, ModuleCachePolicy, TtlUnit,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{info, warn};

// ==========================================
// ANTI-BRUTE-FORCE LOGIN RATE LIMITER
// ==========================================

type AttemptEntry = (u32, Option<Instant>, Instant);

#[derive(Clone, Default)]
pub struct LoginRateLimiter {
    // IP -> (failure_count, locked_until, last_attempt)
    attempts: Arc<Mutex<HashMap<String, AttemptEntry>>>,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn check_allowed(&self, ip: &str) -> Result<(), u64> {
        let guard = self.attempts.lock().await;
        if let Some((_failures, Some(lock_time), _)) = guard.get(ip) {
            let now = Instant::now();
            if now < *lock_time {
                let remaining = (*lock_time - now).as_secs().max(1);
                return Err(remaining);
            }
        }
        Ok(())
    }

    pub async fn record_failure(&self, ip: &str) {
        let mut guard = self.attempts.lock().await;
        let now = Instant::now();
        let entry = guard.entry(ip.to_string()).or_insert((0, None, now));
        entry.0 += 1;
        entry.2 = now;
        if entry.0 >= 5 {
            // Lock out for 60 seconds after 5 failed attempts
            entry.1 = Some(now + Duration::from_secs(60));
        }
    }

    pub async fn record_success(&self, ip: &str) {
        let mut guard = self.attempts.lock().await;
        guard.remove(ip);
    }
}

pub fn extract_client_ip(headers: &HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let t = first.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    if let Some(xrip) = headers.get("x-real-ip").and_then(|h| h.to_str().ok()) {
        let t = xrip.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    "127.0.0.1".to_string()
}

pub fn is_request_secure(headers: &HeaderMap) -> bool {
    if let Some(proto) = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
    {
        if proto.eq_ignore_ascii_case("https") {
            return true;
        }
    }
    false
}

// ==========================================
// ANTI-BAN DOMAIN THROTTLER
// ==========================================

#[derive(Clone, Default)]
pub struct DomainThrottler {
    last_requests: Arc<Mutex<HashMap<String, Instant>>>,
}

impl DomainThrottler {
    pub fn new() -> Self {
        Self {
            last_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn throttle(&self, module_id: &str, min_delay_ms: u64) {
        if min_delay_ms == 0 {
            return;
        }

        let delay = Duration::from_millis(min_delay_ms);
        let mut guard = self.last_requests.lock().await;
        if let Some(last) = guard.get(module_id) {
            let elapsed = last.elapsed();
            if elapsed < delay {
                let wait_time = delay - elapsed;
                drop(guard);
                tokio::time::sleep(wait_time).await;
                let mut guard = self.last_requests.lock().await;
                guard.insert(module_id.to_string(), Instant::now());
                return;
            }
        }
        guard.insert(module_id.to_string(), Instant::now());
    }
}

// ==========================================
// BOOTSTRAP INITIAL ADMIN FROM ENV / SECRET
// ==========================================

pub async fn bootstrap_admin_from_env_or_secret(db: &Database) -> anyhow::Result<()> {
    let count = db.count_users().await?;
    if count > 0 {
        return Ok(());
    }

    let user_opt = std::env::var("FMD_ADMIN_USER").ok();
    let pass_opt = if let Ok(p) = std::env::var("FMD_ADMIN_PASSWORD") {
        Some(p)
    } else if let Ok(file_path) = std::env::var("FMD_ADMIN_PASSWORD_FILE") {
        match std::fs::read_to_string(&file_path) {
            Ok(content) => Some(content.trim().to_string()),
            Err(e) => {
                warn!(
                    "Failed to read FMD_ADMIN_PASSWORD_FILE from {}: {}",
                    file_path, e
                );
                None
            }
        }
    } else {
        None
    };

    if let (Some(username), Some(password)) = (user_opt, pass_opt) {
        let username = username.trim();
        let password = password.trim();
        if !username.is_empty() && !password.is_empty() {
            info!(
                "Bootstrapping initial admin user '{}' from environment/secret...",
                username
            );
            db.create_initial_admin(username, password).await?;
            info!(
                "Initial admin user '{}' bootstrapped successfully",
                username
            );
        }
    } else {
        info!("No admin credentials provided via environment or secret. Interactive setup wizard will be available on Web UI.");
    }

    Ok(())
}

// ==========================================
// AUTH EXTRACTORS & GUARDS
// ==========================================

fn extract_token_from_headers(headers: &HeaderMap) -> Option<String> {
    // 1. Authorization header: "Bearer <token>"
    if let Some(auth_val) = headers.get(AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth_val.strip_prefix("Bearer ") {
            let t = token.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    // 2. Cookie header: "fmd_session=<token>"
    if let Some(cookie_val) = headers.get(COOKIE).and_then(|h| h.to_str().ok()) {
        for cookie in cookie_val.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("fmd_session=") {
                let t = token.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }

    None
}

fn extract_token_from_parts(parts: &Parts) -> Option<String> {
    if let Some(t) = extract_token_from_headers(&parts.headers) {
        return Some(t);
    }
    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                let t = token.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

pub struct RequireAuth(pub fmd_core::AuthContext);

impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        if let Some(token) = extract_token_from_parts(parts) {
            if let Ok(Some(ctx)) = app_state.db.validate_session_or_token(&token).await {
                return Ok(RequireAuth(ctx));
            }
        }

        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Authentification requise" })),
        ))
    }
}

pub struct MaybeAuth(pub Option<fmd_core::AuthContext>);

impl<S> FromRequestParts<S> for MaybeAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        if let Some(token) = extract_token_from_parts(parts) {
            if let Ok(Some(ctx)) = app_state.db.validate_session_or_token(&token).await {
                return Ok(MaybeAuth(Some(ctx)));
            }
        }
        Ok(MaybeAuth(None))
    }
}

pub struct RequireAdmin(pub fmd_core::AuthContext);

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireAuth(ctx) = RequireAuth::from_request_parts(parts, state).await?;
        if !ctx.user.is_admin() {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "Privilèges administrateur requis" })),
            ));
        }
        Ok(RequireAdmin(ctx))
    }
}

// ==========================================
// DTOs & API HANDLERS
// ==========================================

#[derive(Serialize)]
pub struct AuthStatusResponse {
    pub setup_needed: bool,
    pub authenticated: bool,
    pub user: Option<User>,
    pub permissions: Option<UserPermissions>,
}

pub async fn get_auth_status(
    State(state): State<AppState>,
    maybe_auth: MaybeAuth,
) -> Json<AuthStatusResponse> {
    let count = state.db.count_users().await.unwrap_or(0);
    let setup_needed = count == 0;
    match maybe_auth.0 {
        Some(ctx) => Json(AuthStatusResponse {
            setup_needed,
            authenticated: true,
            user: Some(ctx.user),
            permissions: Some(ctx.permissions),
        }),
        None => Json(AuthStatusResponse {
            setup_needed,
            authenticated: false,
            user: None,
            permissions: None,
        }),
    }
}

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

pub async fn auth_setup(
    State(state): State<AppState>,
    req_headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<(HeaderMap, Json<AuthStatusResponse>), (StatusCode, String)> {
    let count = state
        .db
        .count_users()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if count > 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Configuration initiale déjà effectuée".into(),
        ));
    }

    let username = req.username.trim();
    let password = req.password.trim();
    if username.is_empty() || password.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Identifiant ou mot de passe invalide (4 car. min)".into(),
        ));
    }

    let user = state
        .db
        .create_initial_admin(username, password)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erreur création admin: {}", e),
            )
        })?;

    let perms = state
        .db
        .get_user_permissions(&user.id)
        .await
        .unwrap_or_else(|_| UserPermissions::admin_defaults(&user.id));
    let raw_token = state
        .db
        .create_session(&user.id, None, None, 30)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let is_secure = is_request_secure(&req_headers);
    let cookie_val = format!(
        "fmd_session={}; HttpOnly; Path=/; SameSite=Lax; Max-Age=2592000{}",
        raw_token,
        if is_secure { "; Secure" } else { "" }
    );
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&cookie_val).unwrap());

    Ok((
        headers,
        Json(AuthStatusResponse {
            setup_needed: false,
            authenticated: true,
            user: Some(user),
            permissions: Some(perms),
        }),
    ))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn auth_login(
    State(state): State<AppState>,
    req_headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<serde_json::Value>), (StatusCode, String)> {
    let client_ip = extract_client_ip(&req_headers);

    // Rate Limiting anti-brute force check
    if let Err(remaining_secs) = state.login_rate_limiter.check_allowed(&client_ip).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "Trop de tentatives échouées. Veuillez patienter {} secondes.",
                remaining_secs
            ),
        ));
    }

    let username = req.username.trim();
    let user_opt = state
        .db
        .get_user_by_username(username)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user = match user_opt {
        Some(u) if u.is_active => u,
        _ => {
            state.login_rate_limiter.record_failure(&client_ip).await;
            return Err((
                StatusCode::UNAUTHORIZED,
                "Identifiant ou mot de passe incorrect".into(),
            ));
        }
    };

    let pwd_hash = user.password_hash.as_deref().unwrap_or_default();
    if !auth::verify_password(&req.password, pwd_hash) {
        state.login_rate_limiter.record_failure(&client_ip).await;
        return Err((
            StatusCode::UNAUTHORIZED,
            "Identifiant ou mot de passe incorrect".into(),
        ));
    }

    // Login succeeded, clear failed attempt counters
    state.login_rate_limiter.record_success(&client_ip).await;

    let _ = state.db.update_user_last_login(&user.id).await;
    let raw_token = state
        .db
        .create_session(&user.id, None, None, 30)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let perms = state
        .db
        .get_user_permissions(&user.id)
        .await
        .unwrap_or_else(|_| UserPermissions::user_defaults(&user.id));

    let is_secure = is_request_secure(&req_headers);
    let cookie_val = format!(
        "fmd_session={}; HttpOnly; Path=/; SameSite=Lax; Max-Age=2592000{}",
        raw_token,
        if is_secure { "; Secure" } else { "" }
    );
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&cookie_val).unwrap());

    Ok((
        headers,
        Json(serde_json::json!({
            "status": "ok",
            "token": raw_token,
            "user": user,
            "permissions": perms
        })),
    ))
}

pub async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (HeaderMap, Json<serde_json::Value>) {
    if let Some(token) = extract_token_from_headers(&headers) {
        let _ = state.db.delete_session(&token).await;
    }

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        SET_COOKIE,
        HeaderValue::from_static("fmd_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0"),
    );

    (resp_headers, Json(serde_json::json!({ "status": "ok" })))
}

pub async fn auth_me(RequireAuth(ctx): RequireAuth) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "user": ctx.user,
        "permissions": ctx.permissions,
        "is_api_token": ctx.is_api_token
    }))
}

// API Tokens
pub async fn list_api_tokens(
    State(state): State<AppState>,
    RequireAuth(ctx): RequireAuth,
) -> Result<Json<Vec<auth::ApiToken>>, (StatusCode, String)> {
    let tokens = state
        .db
        .list_api_tokens_for_user(&ctx.user.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(tokens))
}

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub expires_in_days: Option<i64>,
}

pub async fn create_api_token_handler(
    State(state): State<AppState>,
    RequireAuth(ctx): RequireAuth,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let (raw_token, token_meta) = state
        .db
        .create_api_token(&ctx.user.id, &req.name, req.expires_in_days)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "token": raw_token,
        "info": token_meta
    })))
}

pub async fn delete_api_token_handler(
    State(state): State<AppState>,
    RequireAuth(ctx): RequireAuth,
    Path(token_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .db
        .delete_api_token(&ctx.user.id, &token_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ==========================================
// ADMIN USER MANAGEMENT
// ==========================================

pub async fn admin_list_users(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let users = state
        .db
        .list_users_with_permissions()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let res = users
        .into_iter()
        .map(|(u, p)| {
            serde_json::json!({
                "user": u,
                "permissions": p
            })
        })
        .collect();
    Ok(Json(res))
}

#[derive(Deserialize)]
pub struct AdminCreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    pub permissions: Option<UserPermissions>,
}

pub async fn admin_create_user(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(req): Json<AdminCreateUserRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let role = Role::from_str(&req.role);
    let user = state
        .db
        .create_user(
            &req.username,
            Some(&req.password),
            role,
            req.permissions,
            None,
            None,
        )
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let perms = state
        .db
        .get_user_permissions(&user.id)
        .await
        .unwrap_or_else(|_| UserPermissions::user_defaults(&user.id));
    Ok(Json(serde_json::json!({
        "user": user,
        "permissions": perms
    })))
}

#[derive(Deserialize)]
pub struct AdminUpdateUserRequest {
    pub role: Option<String>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
    pub permissions: Option<UserPermissions>,
}

pub async fn admin_update_user(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(user_id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let role = req.role.as_deref().map(Role::from_str);
    state
        .db
        .update_user(
            &user_id,
            role,
            req.is_active,
            req.password.as_deref().filter(|p| !p.is_empty()),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(perms) = req.permissions {
        state
            .db
            .update_user_permissions(&user_id, &perms)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

pub async fn admin_delete_user(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if admin.0.user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Impossible de supprimer son propre compte administrateur".into(),
        ));
    }
    state
        .db
        .delete_user(&user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ==========================================
// ADMIN CACHE POLICIES MANAGEMENT
// ==========================================

pub async fn admin_list_cache_policies(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> Result<Json<Vec<ModuleCachePolicy>>, (StatusCode, String)> {
    let policies = state
        .db
        .list_module_cache_policies()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(policies))
}

#[derive(Deserialize)]
pub struct SetPolicyRequest {
    pub module_id: String,
    pub ttl_value: u32,
    pub ttl_unit: String,
    pub min_delay_ms: u64,
}

pub async fn admin_set_cache_policy(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(req): Json<SetPolicyRequest>,
) -> Result<Json<ModuleCachePolicy>, (StatusCode, String)> {
    let policy = ModuleCachePolicy {
        module_id: req.module_id,
        ttl_value: req.ttl_value,
        ttl_unit: TtlUnit::from_str_loose(&req.ttl_unit),
        min_delay_ms: req.min_delay_ms,
        updated_at: Utc::now(),
    };
    state
        .db
        .set_module_cache_policy(&policy)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(policy))
}

pub async fn admin_clear_manga_cache(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(manga_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .db
        .delete_manga_cache(&manga_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "cleared" })))
}
