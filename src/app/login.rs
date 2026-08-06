use super::{App, send_event};
use crate::event::AuthEvent;
use crate::state::Page;

use tokio::time::{Duration, sleep};

impl App {
    pub(super) fn handle_login(&mut self) {
        let login = &mut self.state.navigation.login;
        login.loading = true;
        login.error = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();

        tokio::spawn(async move {
            match service.login_qr_create().await {
                Ok((url, key)) => {
                    send_event(&sender, AuthEvent::QRCreated { url, key }.into());
                }
                Err(e) => {
                    send_event(&sender, AuthEvent::Error(e.to_string()).into());
                }
            }
        });
    }

    pub(super) fn handle_login_success(&mut self, info: ncm_api::LoginInfo) {
        self.toast(format!("登录成功: {}", info.nickname));
        self.state.navigation.login.loading = false;
        self.state.navigation.user = Some(info);
        self.service.client().flush_cookies();
        if self.state.navigation.page == Page::Login {
            self.navigate_to_main();
        }
    }

    pub(super) fn handle_login_error(&mut self, e: String) {
        self.toast(format!("登录失败: {}", e));
        self.state.navigation.login.loading = false;
        self.state.navigation.login.error = Some(e);
    }

    pub(super) fn handle_qr_created(&mut self, url: String, key: String) {
        self.state.navigation.login.loading = false;
        self.state.navigation.login.qr_url = url;
        self.state.navigation.login.qr_key = key.clone();
        self.state.navigation.login.qr_status_text = "等待扫码...".to_string();

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let mut scanned = false;
            for _ in 0..150 {
                sleep(Duration::from_secs(2)).await;
                match service.login_qr_check(&key).await {
                    Ok(resp) => match resp.code {
                        803 => {
                            match service.login_status().await {
                                Ok(info) => {
                                    send_event(&sender, AuthEvent::Success(info).into());
                                }
                                Err(e) => {
                                    send_event(&sender, AuthEvent::Error(e.to_string()).into());
                                }
                            }
                            return;
                        }
                        800 => {
                            send_event(
                                &sender,
                                AuthEvent::Error("二维码已过期，请重新生成".to_string()).into(),
                            );
                            return;
                        }
                        802 if !scanned => {
                            scanned = true;
                            send_event(
                                &sender,
                                AuthEvent::QRStatus("已扫码，请在手机上确认...".to_string()).into(),
                            );
                        }
                        802 => {}
                        _ => {}
                    },
                    Err(e) => {
                        send_event(&sender, AuthEvent::Error(e.to_string()).into());
                        return;
                    }
                }
            }
            send_event(&sender, AuthEvent::Error("登录超时".to_string()).into());
        });
    }

    pub(super) fn handle_qr_status(&mut self, text: String) {
        self.state.navigation.login.qr_status_text = text;
    }
}
