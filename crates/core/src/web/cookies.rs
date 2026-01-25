use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

const SESSION_COOKIE_NAME: &str = "thunderbot_session";
const NONCE_SIZE: usize = 12;

#[derive(Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
    pub exp: i64,
}

impl UserSession {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.exp
    }
}

pub struct SessionCookie {
    encryption_key: Vec<u8>,
}

impl SessionCookie {
    pub fn new() -> Result<Self> {
        let key_hex = std::env::var("DASHBOARD_TOKEN").map_err(|_| anyhow::anyhow!("DASHBOARD_TOKEN must be set"))?;
        let encryption_key = hex::decode(&key_hex)
            .map_err(|e| anyhow::anyhow!("Invalid DASHBOARD_TOKEN: must be hex-encoded 32 bytes: {}", e))?;

        if encryption_key.len() != 32 {
            anyhow::bail!(
                "Invalid DASHBOARD_TOKEN: must be 32 bytes (64 hex chars), got {} bytes",
                encryption_key.len()
            );
        }

        Ok(Self { encryption_key })
    }

    pub fn encrypt_session(&self, session: &UserSession) -> Result<String> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

        let json_bytes =
            serde_json::to_vec(session).map_err(|e| anyhow::anyhow!("Failed to serialize session: {}", e))?;

        let nonce_bytes = rand::random::<[u8; NONCE_SIZE]>();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, json_bytes.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to encrypt session: {}", e))?;

        let combined: Vec<u8> = [nonce.to_vec(), ciphertext].concat();

        Ok(STANDARD.encode(combined))
    }

    pub fn decrypt_session(&self, encrypted: &str) -> Result<UserSession> {
        let combined = STANDARD
            .decode(encrypted)
            .map_err(|e| anyhow::anyhow!("Invalid base64 in cookie: {}", e))?;

        if combined.len() < NONCE_SIZE {
            anyhow::bail!("Cookie data too short");
        }

        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

        let decrypted_bytes = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Failed to decrypt session: {}", e))?;

        let session: UserSession = serde_json::from_slice(&decrypted_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize session: {}", e))?;

        Ok(session)
    }
}

pub fn set_session_cookie(cookies: &mut Vec<(String, String)>, session: &UserSession) -> Result<()> {
    let cookie_mgr = SessionCookie::new()?;
    let encrypted = cookie_mgr.encrypt_session(session)?;

    cookies.push((
        SESSION_COOKIE_NAME.to_string(),
        format!("{}; HttpOnly; Secure; SameSite=Strict; Max-Age=28800", encrypted),
    ));

    Ok(())
}

pub fn clear_session_cookie(cookies: &mut Vec<(String, String)>) {
    cookies.push((SESSION_COOKIE_NAME.to_string(), "".to_string()));
}
