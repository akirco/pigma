use std::future::Future;
use std::sync::Arc;

use crate::{
    input,
    state::{App, LoginMethod, Page, parse_lyric_lines},
    types::{ApiEndpoint, AppEvent, CommandPanelAction, ContentState, Event, TableMode},
};
use crossterm::event::Event as CrosstermEvent;
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

use crate::state::{LogLevel, SplashLogEntry};

fn send_event(tx: &mpsc::UnboundedSender<Event>, event: Event) {
    if tx.send(event).is_err() {
        log::error!("Failed to send event: receiver dropped");
    }
}

impl App {
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.start_splash_boot();
        while self.state.running {
            terminal.draw(|frame| self.draw(frame))?;

            match tokio::time::timeout(Duration::from_millis(32), self.handle_events()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => {}
            }

            if self.state.splash.boot_complete && self.state.navigation.page == Page::Splash {
                if self.state.offline {
                    self.navigate_to_local();
                } else if self.api.is_logged_in() {
                    self.navigate_to_main();
                    let api = self.api.clone();
                    let sender = self.state.events.sender();
                    tokio::spawn(async move {
                        match api.login_status().await {
                            Ok(info) => {
                                if sender
                                    .send(Event::App(AppEvent::LoginSuccess(info)))
                                    .is_err()
                                {
                                    log::error!("Failed to send LoginSuccess: receiver dropped");
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to get login status: {e}");
                            }
                        }
                    });
                } else {
                    self.state.navigation.page = Page::Login;
                }
            }
        }
        Ok(())
    }

    fn start_splash_boot(&self) {
        let sender = self.state.events.sender();
        let api = self.api.clone();
        let music_dir = dirs::home_dir().unwrap_or_default().join("Music");

        tokio::spawn(async move {
            let send = |progress: f64, text: &str, level: LogLevel| {
                send_event(
                    &sender,
                    Event::App(AppEvent::SplashTick {
                        progress,
                        log: Some(SplashLogEntry {
                            time: crate::utils::clock_time(),
                            text: text.to_string(),
                            level,
                        }),
                    }),
                );
            };

            send(0.05, "Initializing engine...", LogLevel::Success);

            send(0.10, "Checking network connectivity...", LogLevel::Info);
            let online = check_network(&api).await;

            if online {
                send(
                    0.25,
                    "Network: connected to music.163.com",
                    LogLevel::Success,
                );

                send(0.35, "Loading user session...", LogLevel::Info);
                if api.is_logged_in() {
                    send(0.45, "Session: cookies found", LogLevel::Success);
                } else {
                    send(0.45, "Session: not logged in", LogLevel::Info);
                }
            } else {
                send_event(&sender, Event::App(AppEvent::SetOffline));
                send(0.25, "Network: offline, offline mode", LogLevel::Warning);
            }

            send(
                0.50,
                &format!("Scanning local music: {}", music_dir.display()),
                LogLevel::Info,
            );
            let local_songs = crate::playback::scan_local_music(&music_dir);
            let count = local_songs.len();
            send(
                0.80,
                &format!("Local music: {} tracks found", count),
                LogLevel::Success,
            );
            send_event(&sender, Event::App(AppEvent::LocalMusicLoaded(local_songs)));

            send(0.98, "Ready.", LogLevel::Success);
            send_event(
                &sender,
                Event::App(AppEvent::SplashTick {
                    progress: 1.0,
                    log: None,
                }),
            );
        });
    }

    fn navigate_to_local(&mut self) {
        self.state.navigation.page = Page::Main;
        self.state.navigation.nav.focus_section = 1;
        if let Some(s) = self.state.navigation.nav.sections.get(1)
            && let Some(i) = s.items.iter().position(|item| item.name == "本地音乐")
        {
            self.state.navigation.nav.section_states[1].select(Some(i));
        }
        self.state
            .navigation
            .set_content(self.state.local_music.clone());
        self.state.navigation.nav.subtitle = Some("本地音乐".into());
        self.state.navigation.content_selected = 0;
    }

    fn navigate_to_main(&mut self) {
        self.state.navigation.page = Page::Main;

        let api = self
            .state
            .navigation
            .nav
            .sections
            .first()
            .and_then(|s| s.items.first())
            .and_then(|i| i.api.clone());
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, Event::App(AppEvent::NavSelect(api)));
        }
    }

    async fn handle_events(&mut self) -> color_eyre::Result<()> {
        match self.state.events.next().await? {
            Event::Crossterm(event) => match event {
                CrosstermEvent::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                    input::handle_key_events(self, key)?
                }
                CrosstermEvent::Mouse(mouse) => {
                    input::handle_mouse_event(self, mouse.kind);
                }
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
                e => self.handle_app_event(e)?,
            },
        }
        Ok(())
    }

    fn handle_app_event(&mut self, event: AppEvent) -> color_eyre::Result<()> {
        match event {
            AppEvent::Quit => self.quit(),
            AppEvent::SplashTick { progress, log } => self.handle_splash_tick(progress, log),
            AppEvent::Login => self.handle_login(),
            AppEvent::LoginSuccess(info) => self.handle_login_success(info),
            AppEvent::LoginError(e) => self.handle_login_error(e),
            AppEvent::CaptchaSent => self.handle_captcha_sent(),
            AppEvent::QRCreated { url, key } => self.handle_qr_created(url, key),
            AppEvent::QRStatus(text) => self.handle_qr_status(text),
            AppEvent::NavSelect(api_str) => self.handle_nav_select(api_str)?,
            AppEvent::BreadcrumbSet(name) => self.handle_breadcrumb(name),
            AppEvent::ContentLoaded(content) => self.handle_content_loaded(content),
            AppEvent::PlaylistSelect(id) => self.handle_playlist_select(id),
            AppEvent::SongPlay(id) => self.handle_song_play(id),
            AppEvent::PlaybackStarted => self.handle_playback_started(),
            AppEvent::PlaybackProgress { position, total } => {
                self.playback.on_playback_progress(position, total);
            }
            AppEvent::PlaybackFinished => {
                let song_info = self
                    .playback
                    .state
                    .current_song
                    .as_ref()
                    .map(|s| (s.id, s.duration));
                let progress = self.playback.state.progress;
                self.playback.on_playback_finished();
                if let Some((song_id, duration)) = song_info {
                    let time_ms = (duration as f64 * progress) as u64;
                    let api = self.api.clone();
                    tokio::spawn(async move {
                        if let Err(e) = api.report_play(song_id, time_ms, None).await {
                            log::error!("Failed to report play for {song_id}: {e}");
                        }
                    });
                }
            }
            AppEvent::PlaybackError(e) => self.playback.on_playback_error(e),
            AppEvent::LyricsLoaded {
                song_id,
                lyrics,
                translated_lyrics,
            } => self
                .playback
                .on_lyrics_loaded(song_id, lyrics, translated_lyrics),
            AppEvent::HeartbeatSong(song) => self.playback.play_heartbeat_song(song),
            AppEvent::HeartbeatFallback => self.playback.on_heartbeat_fallback(),
            AppEvent::SearchSong(keyword) => self.handle_search_song(keyword),
            AppEvent::LocalMusicLoaded(songs) => {
                self.state.local_music = ContentState::Songs(songs);
            }
            AppEvent::SetOffline => self.state.offline = true,
            AppEvent::Navigate(page) => self.state.navigation.page = page,
            AppEvent::CommandPanel(action) => self.handle_command_panel(action),
            AppEvent::SearchActivated => self.handle_search_activate(),
            AppEvent::SearchDeactivated => self.handle_search_deactivate(),
            AppEvent::ToggleBordered => self.state.bordered = !self.state.bordered,
            AppEvent::ExecuteCommand(action) => self.execute_command(action),
            AppEvent::ContentRestore => self.handle_content_restore(),
            AppEvent::CellAction(row, col) => self.handle_cell_action(row, col)?,
        }
        Ok(())
    }

    fn handle_splash_tick(&mut self, progress: f64, log: Option<SplashLogEntry>) {
        self.state.splash.progress = progress;
        if let Some(entry) = log {
            self.state.splash.logs.push(entry);
        }
        if progress >= 1.0 {
            self.state.splash.status = "READY".to_string();
            self.state.splash.boot_complete = true;
        } else {
            let new_status = splash_status(progress);
            if self.state.splash.status != new_status {
                self.state.splash.status = new_status.to_string();
            }
        }
    }

    fn handle_login(&mut self) {
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

    fn handle_login_success(&mut self, info: ncm_api::LoginInfo) {
        self.state.navigation.login.loading = false;
        self.state.navigation.user = Some(info);
        self.api.flush_cookies();
        if self.state.navigation.page == Page::Login {
            self.navigate_to_main();
        }
    }

    fn handle_login_error(&mut self, e: String) {
        self.state.navigation.login.loading = false;
        self.state.navigation.login.error = Some(e);
    }

    fn handle_captcha_sent(&mut self) {
        self.state.navigation.login.loading = false;
        self.state.navigation.login.captcha_sent = true;
        self.state.navigation.login.error = None;
    }

    fn handle_qr_created(&mut self, url: String, key: String) {
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

    fn handle_qr_status(&mut self, text: String) {
        self.state.navigation.login.qr_status_text = text;
    }

    fn handle_nav_select(&mut self, api_str: String) -> color_eyre::Result<()> {
        let api = match ApiEndpoint::parse(&api_str) {
            Some(ep) => ep,
            None => {
                self.state
                    .navigation
                    .set_content(ContentState::Error(format!("未知: {api_str}")));
                return Ok(());
            }
        };

        if api == ApiEndpoint::LocalMusic {
            self.state
                .navigation
                .set_content(self.state.local_music.clone());
            self.state.navigation.content_selected = 0;
            return Ok(());
        }
        if api == ApiEndpoint::Download {
            self.state.navigation.set_content(ContentState::Empty);
            self.state.navigation.content_selected = 0;
            return Ok(());
        }
        // Check disk cache for cachable APIs
        let ttl = self.config.content_cache_ttl;
        if ttl > 0
            && api != ApiEndpoint::Search
            && let Some(cached) = self.playback.source.cache.load_content_cache(&api_str, ttl)
        {
            self.state.navigation.set_content(cached);
            self.state.navigation.content_selected = 0;
            return Ok(());
        }

        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = None;
        if api == ApiEndpoint::Search {
            self.state.navigation.nav.subtitle = Some("热搜榜".into());
        }
        let cache = self.playback.source.cache.clone();
        let api_client = self.api.clone();
        let sender = self.state.events.sender();
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);

        tokio::spawn(async move {
            let result = match api {
                ApiEndpoint::RecommendResource => api_client
                    .recommend_resource()
                    .await
                    .map(ContentState::SongLists),
                ApiEndpoint::Toplist => api_client.toplist().await.map(ContentState::TopLists),
                ApiEndpoint::TopSongList => api_client
                    .top_song_list("全部", "hot", 0, 50)
                    .await
                    .map(ContentState::SongLists),
                ApiEndpoint::UserRadioSublist => api_client
                    .user_radio_sublist(0, 50)
                    .await
                    .map(ContentState::SongLists),
                ApiEndpoint::RecommendSongs => {
                    api_client.recommend_songs().await.map(ContentState::Songs)
                }
                ApiEndpoint::UserCloudDisk => {
                    api_client.user_cloud_disk().await.map(ContentState::Songs)
                }
                ApiEndpoint::Recent => api_client.recent_songs(100).await.map(ContentState::Songs),
                ApiEndpoint::LikedSongs => {
                    if let Some(uid) = uid {
                        api_client.liked_songs(uid).await.map(ContentState::Songs)
                    } else {
                        Ok(ContentState::Error("未登录".into()))
                    }
                }
                ApiEndpoint::UserSongList => {
                    if let Some(uid) = uid {
                        api_client
                            .user_song_list(uid, 0, 50)
                            .await
                            .map(ContentState::SongLists)
                    } else {
                        Ok(ContentState::Error("未登录".into()))
                    }
                }
                ApiEndpoint::Search => api_client.search_hot().await.map(|items| {
                    ContentState::HotSearch(items.iter().map(|h| h.keyword.clone()).collect())
                }),
                ApiEndpoint::Download => Ok(ContentState::Empty),
                ApiEndpoint::LocalMusic => unreachable!(),
                ApiEndpoint::TopSingers => api_client
                    .top_artists(0, 50)
                    .await
                    .map(ContentState::Singers),
            };

            let state = match result {
                Ok(content) => content,
                Err(e) => ContentState::Error(e.to_string()),
            };

            // Save to disk cache (skip Search — results change too quickly)
            if ttl > 0 && api != ApiEndpoint::Search {
                cache.save_content_cache(&api_str, &state);
            }

            send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
        });
        Ok(())
    }

    fn handle_breadcrumb(&mut self, name: String) {
        self.state.navigation.nav.subtitle = Some(name);
    }

    fn handle_content_loaded(&mut self, content: ContentState) {
        self.state.navigation.set_content(content);
        self.state.navigation.content_selected = 0;
        self.state.navigation.table_mode = TableMode::Row;
        self.state.navigation.content_column_selected = 0;
    }

    fn handle_playlist_select(&mut self, id: u64) {
        self.playback.set_playlist_id(id);
        self.state.navigation.previous_content = Some(std::mem::replace(
            &mut self.state.navigation.content,
            ContentState::Loading,
        ));
        *self.state.navigation.content_rows_cache.borrow_mut() = None;
        self.state.navigation.content_selected = 0;
        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let result = api.song_list_detail(id).await;
            let (state, name) = match result {
                Ok(detail) => (ContentState::Songs(detail.songs), Some(detail.name)),
                Err(e) => (ContentState::Error(e.to_string()), None),
            };
            send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
            if let Some(name) = name {
                send_event(&sender, Event::App(AppEvent::BreadcrumbSet(name)));
            }
        });
    }

    fn handle_song_play(&mut self, id: u64) {
        if let Some(current) = &self.playback.state.current_song
            && current.id == id
            && self.playback.state.playing
        {
            self.playback.toggle_pause();
            return;
        }
        if let ContentState::Songs(songs) = &self.state.navigation.content
            && let Some(pos) = songs.iter().position(|s| s.id == id)
        {
            self.playback.append_and_play(songs, pos);
        }
    }

    fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        if let Some(song) = &self.playback.state.current_song {
            let song_id = song.id;
            let cache = self.playback.source.cache.clone();
            let api = self.api.clone();
            let sender = self.state.events.sender();

            // Check disk cache first
            if let Some(cached) = cache.load_lyrics_cache(song_id) {
                let lyric_lines = parse_lyric_lines(&cached.lyric);
                let tlyric_lines = parse_lyric_lines(&cached.tlyric);
                send_event(
                    &sender,
                    Event::App(AppEvent::LyricsLoaded {
                        song_id,
                        lyrics: lyric_lines,
                        translated_lyrics: tlyric_lines,
                    }),
                );
                return;
            }

            tokio::spawn(async move {
                match api.song_lyric(song_id).await {
                    Ok(lyrics) => {
                        cache.save_lyrics_cache(song_id, &lyrics);
                        let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                        let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                        send_event(
                            &sender,
                            Event::App(AppEvent::LyricsLoaded {
                                song_id,
                                lyrics: lyric_lines,
                                translated_lyrics: tlyric_lines,
                            }),
                        );
                    }
                    Err(e) => {
                        log::error!("Failed to fetch lyrics for {song_id}: {e}");
                    }
                }
            });
        }
    }

    fn handle_search_song(&mut self, keyword: String) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = Some(format!("搜索: {keyword}"));
        self.state.navigation.content_selected = 0;
        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let result = api.search_song(&keyword, 0, 50).await;
            let state = match result {
                Ok(r) => ContentState::Songs(r.songs),
                Err(e) => ContentState::Error(e.to_string()),
            };
            send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
        });
    }

    fn handle_command_panel(&mut self, action: CommandPanelAction) {
        let panel = &mut self.state.command_panel;
        match action {
            CommandPanelAction::Open => {
                panel.open = true;
                panel.selected = 0;
            }
            CommandPanelAction::Close => panel.back(),
            CommandPanelAction::Previous => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    panel.selected = (panel.selected + len - 1) % len;
                }
            }
            CommandPanelAction::Next => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    panel.selected = (panel.selected + 1) % len;
                }
            }
            CommandPanelAction::Select => {
                let action = panel.enter();
                if action.is_some() {
                    panel.open = false;
                }
                if let Some(action) = action {
                    self.execute_command(action);
                }
            }
        }
    }

    fn handle_search_activate(&mut self) {
        let nav = &mut self.state.navigation;
        nav.search.active = true;
        nav.search.input = crate::ui::text_input::TextInput::new();
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;
        nav.search.unfiltered_songs_lower = None;
        nav.nav.subtitle = None;
        nav.content_selected = 0;

        nav.previous_api = {
            let n = &nav.nav;
            n.section_states
                .get(n.focus_section)
                .and_then(|st| st.selected())
                .and_then(|i| n.sections.get(n.focus_section)?.items.get(i))
                .and_then(|item| item.api.clone())
        };

        nav.nav.restore_focus_by_api("search");

        nav.previous_content = Some(std::mem::replace(&mut nav.content, ContentState::Empty));
        *nav.content_rows_cache.borrow_mut() = None;

        let api = nav
            .nav
            .section_states
            .get(nav.nav.focus_section)
            .and_then(|st| st.selected())
            .and_then(|i| nav.nav.sections.get(nav.nav.focus_section)?.items.get(i))
            .and_then(|item| item.api.as_ref());
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, Event::App(AppEvent::NavSelect(api.clone())));
        }
    }

    fn handle_search_deactivate(&mut self) {
        let nav = &mut self.state.navigation;
        if nav.search.filter_queue_only {
            nav.search.filter_queue_only = false;
            if let Some(songs) = nav.search.unfiltered_songs.take() {
                self.playback.queue.songs = songs;
            }
            nav.search.unfiltered_songs_lower = None;
        } else if let Some(prev) = nav.previous_content.take() {
            nav.set_content(prev);
            if let Some(ref api) = nav.previous_api.take() {
                nav.nav.restore_focus_by_api(api);
            }
        }
        nav.search.active = false;
        nav.search.input = crate::ui::text_input::TextInput::new();
        nav.nav.subtitle = None;
    }

    fn handle_content_restore(&mut self) {
        let nav = &mut self.state.navigation;
        if let Some(prev) = nav.previous_content.take() {
            nav.set_content(prev);
            nav.nav.subtitle = None;
            if let Some(ref api) = nav.previous_api.take() {
                nav.nav.restore_focus_by_api(api);
            }
        }
    }

    fn navigate_to_entity<F, Fut>(&mut self, name: String, api_call: F)
    where
        F: FnOnce(Arc<ncm_api::NcmClient>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<ncm_api::SongInfo>, ncm_api::NcmError>> + Send,
    {
        let prev_content = self.state.navigation.content.clone();
        let prev_api = self.state.navigation.nav.section_states
            [self.state.navigation.nav.focus_section]
            .selected()
            .and_then(|i| {
                self.state.navigation.nav.sections[self.state.navigation.nav.focus_section]
                    .items
                    .get(i)
            })
            .and_then(|item| item.api.clone());
        self.state.navigation.previous_content = Some(prev_content);
        self.state.navigation.previous_api = prev_api;
        self.state.navigation.set_content(ContentState::Loading);

        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let result = api_call(api).await;
            let state = match result {
                Ok(songs) => ContentState::Songs(songs),
                Err(e) => ContentState::Error(e.to_string()),
            };
            let _ = sender.send(Event::App(AppEvent::ContentLoaded(state)));
            let _ = sender.send(Event::App(AppEvent::BreadcrumbSet(name)));
        });
    }

    fn handle_cell_action(&mut self, row: usize, col: usize) -> color_eyre::Result<()> {
        let columns = self
            .config
            .columns
            .for_content(&self.state.navigation.content, None)
            .to_vec();
        let column = match columns.get(col) {
            Some(c) => c.clone(),
            None => return Ok(()),
        };
        let field = column.field.clone();

        match (&self.state.navigation.content, field.as_str()) {
            (ContentState::Songs(songs), "album") => {
                if let Some(song) = songs.get(row) {
                    let album_id = song.album_id;
                    let name = format!("{}: {}", column.header, song.album);
                    self.navigate_to_entity(name, move |api| async move {
                        api.album(album_id).await.map(|d| d.songs)
                    });
                }
            }
            (ContentState::Songs(songs), "singer") => {
                if let Some(song) = songs.get(row) {
                    let artist_id = song.artist_id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, song.singer);
                    self.navigate_to_entity(name, move |api| async move {
                        api.singer_songs(artist_id).await
                    });
                }
            }
            (ContentState::Singers(singers), "name") => {
                if let Some(singer) = singers.get(row) {
                    let artist_id = singer.id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, singer.name);
                    self.navigate_to_entity(name, move |api| async move {
                        api.singer_songs(artist_id).await
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        crate::ui::draw(frame, self);
    }
}

fn splash_status(progress: f64) -> &'static str {
    if progress < 0.3 {
        "INITIALIZING SYSTEM..."
    } else if progress < 0.6 {
        "CONNECTING TO SERVER..."
    } else if progress < 0.9 {
        "LOADING LIBRARY..."
    } else {
        "READY"
    }
}

async fn check_network(_api: &ncm_api::NcmClient) -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect("music.163.com:443").await.is_ok()
}
