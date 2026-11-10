use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        s.parse().unwrap_or(Role::User)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

impl std::str::FromStr for Role {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            _ => Ok(Role::User),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub role: Role,
    pub is_active: bool,
    pub sso_provider: Option<String>,
    pub sso_subject: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role.is_admin()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPermissions {
    pub user_id: String,
    pub can_download: bool,
    pub can_manage_favorites: bool,
    pub can_trigger_catalog_sync: bool,
    pub can_force_refresh: bool,
    pub can_view_modules: bool,
}

impl Default for UserPermissions {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            can_download: true,
            can_manage_favorites: true,
            can_trigger_catalog_sync: false,
            can_force_refresh: false,
            can_view_modules: true,
        }
    }
}

impl UserPermissions {
    pub fn admin_defaults(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            can_download: true,
            can_manage_favorites: true,
            can_trigger_catalog_sync: true,
            can_force_refresh: true,
            can_view_modules: true,
        }
    }

    pub fn user_defaults(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            can_download: true,
            can_manage_favorites: true,
            can_trigger_catalog_sync: false,
            can_force_refresh: false,
            can_view_modules: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub user: User,
    pub permissions: UserPermissions,
    pub is_api_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Hashes password with Argon2id using OsRng salt
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Argon2 hash error: {}", e))?
        .to_string();
    Ok(password_hash)
}

/// Verifies clear text password against stored Argon2id hash
pub fn verify_password(password: &str, hash: &str) -> bool {
    if let Ok(parsed_hash) = PasswordHash::new(hash) {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    } else {
        false
    }
}

/// Generates a high-entropy random URL-safe token (e.g. for session IDs or API keys)
pub fn generate_secure_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Hashes a token with SHA-256 before storing it in the database
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing_and_verification() {
        let password = "SuperSecretPassword123!";
        let hash = hash_password(password).expect("hashing failed");
        assert_ne!(password, hash);
        assert!(verify_password(password, &hash));
        assert!(!verify_password("WrongPassword!", &hash));
    }

    #[test]
    fn test_token_generation_and_hashing() {
        let token = generate_secure_token();
        assert_eq!(token.len(), 64);
        let hash1 = hash_token(&token);
        let hash2 = hash_token(&token);
        assert_eq!(hash1, hash2);
        assert_ne!(token, hash1);
    }
}
