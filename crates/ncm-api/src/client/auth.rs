use super::NcmClient;
use crate::{encrypt, error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Authentication =====

    /// Log in (automatically detects a phone number or email)
    ///
    /// * `username` — phone number (11 digits) or email
    /// * `password` — password (plain text)
    ///
    /// Consistent with the official client: the password is first MD5-hashed, email goes through
    /// eAPI `/api/w/login`, and phone goes through weapi `/api/w/login/cellphone`
    /// (`/weapi/w/login/cellphone`).
    pub async fn login(&self, username: &str, password: &str) -> Result<LoginInfo, NcmError> {
        let md5_password = encrypt::md5_hex(password);
        let value = if username.len() == 11 && username.parse::<u64>().is_ok() {
            let params = vec![
                ("type", "1"),
                ("https", "true"),
                ("phone", username),
                ("countrycode", "86"),
                ("password", md5_password.as_str()),
                ("remember", "true"),
            ];
            let result = self
                .request_weapi("/api/w/login/cellphone", &params)
                .await?;
            serde_json::from_str(&result)?
        } else {
            let params = serde_json::json!({
                "type": "0",
                "https": "true",
                "username": username,
                "password": md5_password,
                "rememberLogin": "true",
            });
            let result = self.request_eapi_value("/api/w/login", params).await?;
            serde_json::from_str(&result)?
        };
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Phone verification code login
    ///
    /// * `ctcode` — country code (e.g. `86`)
    /// * `phone` — phone number
    /// * `captcha` — verification code
    pub async fn login_cellphone(
        &self,
        ctcode: &str,
        phone: &str,
        captcha: &str,
    ) -> Result<LoginInfo, NcmError> {
        let params = vec![
            ("type", "1"),
            ("https", "true"),
            ("phone", phone),
            ("countrycode", ctcode),
            ("captcha", captcha),
            ("remember", "true"),
        ];
        let result = self
            .request_weapi("/api/w/login/cellphone", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Send an SMS verification code
    ///
    /// * `ctcode` — country code (e.g. `86`)
    /// * `phone` — phone number
    pub async fn captcha(&self, ctcode: &str, phone: &str) -> Result<(), NcmError> {
        let params = vec![("cellphone", phone), ("ctcode", ctcode)];
        let result = self
            .request_weapi("/weapi/sms/captcha/sent", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)
    }

    /// Create a login QR code, returns (QR code URL, unikey)
    pub async fn login_qr_create(&self) -> Result<(String, String), NcmError> {
        let params = vec![("type", "1")];
        let result = self
            .request_weapi("/weapi/login/qrcode/unikey", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let unikey = parse_unikey(&value).map_err(|e| NcmError::parse(e, &value))?;
        let qr_url = format!("https://music.163.com/login?codekey={}", &unikey);
        Ok((qr_url, unikey))
    }

    /// Poll the QR code login status
    ///
    /// * `key` — the unikey returned by `login_qr_create`
    pub async fn login_qr_check(&self, key: &str) -> Result<Msg, NcmError> {
        let params = vec![("type", "1"), ("key", key)];
        let result = self
            .request_weapi("/weapi/login/qrcode/client/login", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the current login status
    pub async fn login_status(&self) -> Result<LoginInfo, NcmError> {
        let result = self.request_weapi("/api/nuser/account/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Log out
    pub async fn logout(&self) -> Result<Msg, NcmError> {
        let result = self.request_weapi("/weapi/logout", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Anonymous registration: obtains an anonymous session cookie (MUSIC_U) as the device
    /// fingerprint, together with `deviceId` it lowers the chance of triggering login risk
    /// control. Note: after success, `is_logged_in()` returns true because of the anonymous
    /// MUSIC_U; do not call this if the anonymous session is not needed (or overwrite it after a real login).
    pub async fn register_anonimous(&self) -> Result<(), NcmError> {
        let device_id = self.with_store(|s| s.device_id().to_string())?;
        let encoded = encrypt::cloudmusic_encode_id(&device_id);
        let username = encrypt::base64_utf8(&format!("{device_id} {encoded}"));
        let params = vec![("username", username.as_str())];
        let result = self
            .request_weapi("/api/register/anonimous", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        Ok(())
    }
}
