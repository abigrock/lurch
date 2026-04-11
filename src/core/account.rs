use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Azure AD client config ───────────────────────────────────────────────────
pub const MS_CLIENT_ID: &str = "afe3f0d1-362f-4414-8352-1758cebf9ffe";
const MS_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MS_SCOPE: &str = "XboxLive.signin offline_access";

const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_AUTH_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

// ── Account model ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub uuid: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    pub skin_url: Option<String>,
    pub active: bool,
    #[serde(default)]
    pub offline: bool,
}

impl Account {
    /// Create an offline/demo account (no Microsoft auth needed)
    pub fn offline(username: String) -> Self {
        // Generate a deterministic offline UUID matching Java's
        // UUID.nameUUIDFromBytes("OfflinePlayer:<name>".getBytes("UTF-8"))
        let input = format!("OfflinePlayer:{username}");
        let digest = md5::compute(input.as_bytes());
        let uuid = uuid::Builder::from_md5_bytes(digest.0)
            .into_uuid()
            .simple()
            .to_string();
        Self {
            uuid,
            username,
            access_token: String::new(),
            refresh_token: String::new(),
            skin_url: None,
            active: false,
            offline: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
}

impl AccountStore {
    pub fn active_account(&self) -> Option<&Account> {
        self.accounts.iter().find(|a| a.active)
    }

    pub fn set_active(&mut self, uuid: &str) {
        for acc in &mut self.accounts {
            acc.active = acc.uuid == uuid;
        }
    }

    pub fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        // If removed the active one, activate the first remaining
        if !self.accounts.iter().any(|a| a.active)
            && let Some(first) = self.accounts.first_mut() {
                first.active = true;
            }
    }

    pub fn add_or_update(&mut self, account: Account) {
        if let Some(existing) = self.accounts.iter_mut().find(|a| a.uuid == account.uuid) {
            existing.username = account.username;
            existing.access_token = account.access_token;
            existing.refresh_token = account.refresh_token;
            existing.skin_url = account.skin_url;
            existing.offline = account.offline;
        } else {
            // Deactivate others if this is the first
            let is_first = self.accounts.is_empty();
            let mut acc = account;
            acc.active = is_first;
            self.accounts.push(acc);
        }
    }

    pub fn load() -> Self {
        let path = Self::store_path();
        match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn store_path() -> PathBuf {
        crate::util::paths::config_dir()
            .map(|d| d.join("accounts.json"))
            .unwrap_or_else(|_| PathBuf::from("lurch_accounts.json"))
    }
}

// ── Microsoft Device Code Flow ───────────────────────────────────────────────

/// Step 1: Request a device code from Microsoft
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Step 2: Poll for token after user completes browser auth
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct MsTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct MsTokenError {
    error: String,
}

/// Request a device code for the user to enter at microsoft.com/devicelogin
pub fn request_device_code() -> anyhow::Result<DeviceCodeResponse> {
    let client = crate::core::http_client();
    let resp = client
        .post(MS_DEVICE_CODE_URL)
        .form(&[("client_id", MS_CLIENT_ID), ("scope", MS_SCOPE)])
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Device code request failed (HTTP {status}): {body}");
    }
    let code: DeviceCodeResponse = serde_json::from_str(&body)?;
    Ok(code)
}

/// Poll for MS token completion. Returns Ok(Some(token)) when done,
/// Ok(None) if still pending, Err if failed/expired.
pub fn poll_device_code_token(device_code: &str) -> anyhow::Result<Option<MsTokenResponse>> {
    let client = crate::core::http_client();
    let resp = client
        .post(MS_TOKEN_URL)
        .form(&[
            ("client_id", MS_CLIENT_ID),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ])
        .send()?;

    let status = resp.status();
    let body = resp.text()?;

    if status.is_success() {
        let token: MsTokenResponse = serde_json::from_str(&body)?;
        return Ok(Some(token));
    }

    // Check if still pending
    if let Ok(err) = serde_json::from_str::<MsTokenError>(&body) {
        if err.error == "authorization_pending" {
            return Ok(None);
        }
        if err.error == "expired_token" {
            anyhow::bail!("Device code expired. Please try again.");
        }
        if err.error == "authorization_declined" {
            anyhow::bail!("Authorization was declined.");
        }
    }

    anyhow::bail!("Token poll failed: {body}")
}

/// Refresh an existing MS token
pub fn refresh_ms_token(refresh_token: &str) -> anyhow::Result<MsTokenResponse> {
    let client = crate::core::http_client();
    let resp = client
        .post(MS_TOKEN_URL)
        .form(&[
            ("client_id", MS_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", MS_SCOPE),
        ])
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Token refresh failed (HTTP {status}): {body}");
    }
    let token: MsTokenResponse = serde_json::from_str(&body)?;
    Ok(token)
}

// ── Xbox Live + XSTS + Minecraft auth chain ──────────────────────────────────

#[derive(Debug, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblXui>,
}

#[derive(Debug, Deserialize)]
struct XblXui {
    uhs: String,
}

/// Authenticate with Xbox Live using the MS access token
fn auth_xbox_live(ms_access_token: &str) -> anyhow::Result<(String, String)> {
    let client = crate::core::http_client();
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let resp = client
        .post(XBL_AUTH_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Xbox Live auth failed (HTTP {status}): {body}");
    }
    let xbl: XblResponse = serde_json::from_str(&body)?;
    let uhs = xbl
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .unwrap_or_default();
    Ok((xbl.token, uhs))
}

/// Get XSTS token from XBL token
fn auth_xsts(xbl_token: &str) -> anyhow::Result<String> {
    let client = crate::core::http_client();
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let resp = client
        .post(XSTS_AUTH_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("XSTS auth failed (HTTP {status}): {body}");
    }
    let xsts: XblResponse = serde_json::from_str(&body)?;
    Ok(xsts.token)
}

/// Exchange XSTS token for Minecraft access token
#[derive(Debug, Deserialize)]
struct McAuthResponse {
    access_token: String,
}

fn auth_minecraft(xsts_token: &str, user_hash: &str) -> anyhow::Result<String> {
    let client = crate::core::http_client();
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
    });

    let resp = client
        .post(MC_AUTH_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Minecraft auth failed (HTTP {status}): {body}");
    }
    let mc: McAuthResponse = serde_json::from_str(&body)?;
    Ok(mc.access_token)
}

/// Fetch Minecraft profile (UUID, username, skin)
#[derive(Debug, Deserialize)]
struct McProfile {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<McSkin>,
}

#[derive(Debug, Deserialize)]
struct McSkin {
    url: String,
}

fn fetch_mc_profile(mc_token: &str) -> anyhow::Result<(String, String, Option<String>)> {
    let client = crate::core::http_client();
    let resp = client
        .get(MC_PROFILE_URL)
        .header("Authorization", format!("Bearer {mc_token}"))
        .send()?;
    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Failed to fetch MC profile (HTTP {status}): {body}");
    }
    let profile: McProfile = serde_json::from_str(&body)?;
    let skin_url = profile.skins.first().map(|s| s.url.clone());
    Ok((profile.id, profile.name, skin_url))
}

// ── Full auth flow ───────────────────────────────────────────────────────────

/// Complete the full auth chain after getting MS tokens.
/// Returns a fully populated Account.
pub fn complete_auth(ms_token: &MsTokenResponse) -> anyhow::Result<Account> {
    // XBL
    let (xbl_token, user_hash) = auth_xbox_live(&ms_token.access_token)?;

    // XSTS
    let xsts_token = auth_xsts(&xbl_token)?;

    // Minecraft
    let mc_token = auth_minecraft(&xsts_token, &user_hash)?;

    // Profile
    let (uuid, username, skin_url) = fetch_mc_profile(&mc_token)?;

    Ok(Account {
        uuid,
        username,
        access_token: mc_token,
        refresh_token: ms_token.refresh_token.clone(),
        skin_url,
        active: false,
        offline: false,
    })
}

/// Refresh an account's tokens. Returns updated Account on success.
pub fn refresh_account(account: &Account) -> anyhow::Result<Account> {
    let ms_token = refresh_ms_token(&account.refresh_token)?;
    let mut updated = complete_auth(&ms_token)?;
    updated.active = account.active;
    Ok(updated)
}
