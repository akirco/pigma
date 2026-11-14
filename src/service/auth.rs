//! Authentication: QR-code login and login-status lookups on `ApiService`.

use super::ApiService;

impl ApiService {
    pub async fn login_qr_create(&self) -> Result<(String, String), ncm_api::NcmError> {
        self.client.login_qr_create().await
    }

    pub async fn login_qr_check(&self, key: &str) -> Result<ncm_api::Msg, ncm_api::NcmError> {
        self.client.login_qr_check(key).await
    }

    pub async fn login_status(&self) -> Result<ncm_api::LoginInfo, ncm_api::NcmError> {
        self.client.login_status().await
    }
}
