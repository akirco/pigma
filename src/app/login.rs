use super::{App, send_event};
use crate::event::{AppEvent, Event};
use crate::state::{LoginMethod, Page};

use tokio::time::{Duration, sleep};

impl App {
    pub(super) fn handle_login(&mut self) {
        let login = &mut self.state.navigation.login;
        login.loading = true;
        login.error = None;

        match login.selected_method {
            LoginMethod::Email => {
                let username = login.username.value.clone();
                let password = login.password.value.clone();
                let api = self.api.clone();
                let sender = self.state.events.sender();

                tokio::spawn(async move {
                    match api.login(&username, &password).await {
                        Ok(info) => {
                            send_event(&sender, Event::App(AppEvent::LoginSuccess(info)));
                        }
                        Err(e) => {
                            send_event(&sender, Event::App(AppEvent::LoginError(e.to_string())));
                        }
                    }
                });
            }
            LoginMethod::Phone => {
                if login.captcha_sent {
                    let phone = login.username.value.clone();
                    let captcha = login.password.value.clone();
                    let api = self.api.clone();
                    let sender = self.state.events.sender();

                    tokio::spawn(async move {
                        match api.login_cellphone("86", &phone, &captcha).await {
                            Ok(info) => {
                                send_event(&sender, Event::App(AppEvent::LoginSuccess(info)));
                            }
                            Err(e) => {
                                send_event(
                                    &sender,
                                    Event::App(AppEvent::LoginError(e.to_string())),
                                );
                            }
                        }
                    });
                } else {
                    let phone = login.username.value.clone();
                    let api = self.api.clone();
                    let sender = self.state.events.sender();

                    tokio::spawn(async move {
                        match api.captcha("86", &phone).await {
                            Ok(()) => {
                                send_event(&sender, Event::App(AppEvent::CaptchaSent));
                            }
                            Err(e) => {
                                send_event(
                                    &sender,
                                    Event::App(AppEvent::LoginError(e.to_string())),
                                );
                            }
                        }
                    });
                }
            }
            LoginMethod::QR => {
                let api = self.api.clone();
                let sender = self.state.events.sender();

                tokio::spawn(async move {
                    match api.login_qr_create().await {
                        Ok((url, key)) => {
                            send_event(&sender, Event::App(AppEvent::QRCreated { url, key }));
                        }
                        Err(e) => {
                            send_event(&sender, Event::App(AppEvent::LoginError(e.to_string())));
                        }
                    }
                });
            }
        }
    }

    pub(super) fn handle_login_success(&mut self, info: ncm_api::LoginInfo) {
        self.toast(format!("登录成功: {}", info.nickname));
        self.state.navigation.login.loading = false;
        self.state.navigation.user = Some(info);
        self.api.flush_cookies();
        if self.state.navigation.page == Page::Login {
            self.navigate_to_main();
        }
    }

    pub(super) fn handle_login_error(&mut self, e: String) {
        self.toast(format!("登录失败: {}", e));
        self.state.navigation.login.loading = false;
        self.state.navigation.login.error = Some(e);
    }

    pub(super) fn handle_captcha_sent(&mut self) {
        self.state.navigation.login.loading = false;
        self.state.navigation.login.captcha_sent = true;
        self.state.navigation.login.error = None;
    }

    pub(super) fn handle_qr_created(&mut self, url: String, key: String) {
        self.state.navigation.login.loading = false;
        self.state.navigation.login.qr_url = url;
        self.state.navigation.login.qr_key = key.clone();
        self.state.navigation.login.qr_status_text = "等待扫码...".to_string();

        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let mut scanned = false;
            for _ in 0..150 {
                sleep(Duration::from_secs(2)).await;
                match api.login_qr_check(&key).await {
                    Ok(resp) => match resp.code {
                        803 => {
                            match api.login_status().await {
                                Ok(info) => {
                                    send_event(&sender, Event::App(AppEvent::LoginSuccess(info)));
                                }
                                Err(e) => {
                                    send_event(
                                        &sender,
                                        Event::App(AppEvent::LoginError(e.to_string())),
                                    );
                                }
                            }
                            return;
                        }
                        800 => {
                            send_event(
                                &sender,
                                Event::App(AppEvent::LoginError(
                                    "二维码已过期，请重新生成".to_string(),
                                )),
                            );
                            return;
                        }
                        802 if !scanned => {
                            scanned = true;
                            send_event(
                                &sender,
                                Event::App(AppEvent::QRStatus(
                                    "已扫码，请在手机上确认...".to_string(),
                                )),
                            );
                        }
                        802 => {}
                        _ => {}
                    },
                    Err(e) => {
                        send_event(&sender, Event::App(AppEvent::LoginError(e.to_string())));
                        return;
                    }
                }
            }
            send_event(
                &sender,
                Event::App(AppEvent::LoginError("登录超时".to_string())),
            );
        });
    }

    pub(super) fn handle_qr_status(&mut self, text: String) {
        self.state.navigation.login.qr_status_text = text;
    }
}
