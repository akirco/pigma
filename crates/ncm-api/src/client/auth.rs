use super::NcmClient;
use crate::{encrypt, error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 认证 =====

    /// 登录（自动识别手机号/邮箱）
    ///
    /// * `username` — 手机号（11 位数字）或邮箱
    /// * `password` — 密码（明文）
    ///
    /// 与官方客户端一致：密码先 MD5，邮箱走 eAPI `/api/w/login`，手机走
    /// weapi `/api/w/login/cellphone`（`/weapi/w/login/cellphone`）。
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

    /// 手机验证码登录
    ///
    /// * `ctcode` — 国家码（如 `86`）
    /// * `phone` — 手机号
    /// * `captcha` — 验证码
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

    /// 发送短信验证码
    ///
    /// * `ctcode` — 国家码（如 `86`）
    /// * `phone` — 手机号
    pub async fn captcha(&self, ctcode: &str, phone: &str) -> Result<(), NcmError> {
        let params = vec![("cellphone", phone), ("ctcode", ctcode)];
        let result = self
            .request_weapi("/weapi/sms/captcha/sent", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)
    }

    /// 创建登录二维码，返回 (二维码 URL, unikey)
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

    /// 轮询二维码登录状态
    ///
    /// * `key` — 由 `login_qr_create` 返回的 unikey
    pub async fn login_qr_check(&self, key: &str) -> Result<Msg, NcmError> {
        let params = vec![("type", "1"), ("key", key)];
        let result = self
            .request_weapi("/weapi/login/qrcode/client/login", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取当前登录状态
    pub async fn login_status(&self) -> Result<LoginInfo, NcmError> {
        let result = self.request_weapi("/api/nuser/account/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 退出登录
    pub async fn logout(&self) -> Result<Msg, NcmError> {
        let result = self.request_weapi("/weapi/logout", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 匿名注册：获取匿名会话 cookie（MUSIC_U）作为设备指纹，配合 `deviceId`
    /// 降低登录风控触发概率。注意：成功后 `is_logged_in()` 会因匿名 MUSIC_U
    /// 返回 true，若不需要匿名会话请不要调用（或在真实登录后覆盖）。
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
