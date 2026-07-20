use std::io::{Stdout, stdout};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, Paragraph},
};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

const SONG_NAME: &str = "Neon Skyline";
const SONG_SINGER: &str = "Aurora Lights";
const SONG_ALBUM: &str = "Midnight Drive";

struct App {
    progress: f64,
    volume: f64,
    paused: bool,
    playing: bool,
    seeking: bool,
    circle: bool,
    cover: StatefulProtocol,
    last_tick: Instant,
}

impl App {
    fn new(picker: &Picker, cover_path: Option<&str>) -> color_eyre::Result<Self> {
        let cover = load_cover(picker, cover_path, true)?;
        Ok(Self {
            progress: 0.37,
            volume: 0.65,
            paused: false,
            playing: true,
            seeking: false,
            circle: true,
            cover,
            last_tick: Instant::now(),
        })
    }

    fn tick(&mut self) {
        if self.playing && !self.paused && !self.seeking {
            let now = Instant::now();
            let dt = now.duration_since(self.last_tick).as_secs_f64();
            self.last_tick = now;
            self.progress = (self.progress + dt / 217.0).rem_euclid(1.0);
        }
    }
}

fn load_cover(
    picker: &Picker,
    path: Option<&str>,
    circle: bool,
) -> color_eyre::Result<StatefulProtocol> {
    let dyn_img: DynamicImage = match path {
        Some(p) => image::open(p)?,
        None => {
            if std::path::Path::new("./imgs/1.jpg").exists() {
                image::open("./imgs/1.jpg")?
            } else {
                DynamicImage::ImageRgba8(synthesize_cover())
            }
        }
    };

    let img: DynamicImage = if circle {
        let (w, h) = dyn_img.dimensions();
        let size = w.min(h);
        let x = (w - size) / 2;
        let y = (h - size) / 2;
        let mut square = dyn_img.crop_imm(x, y, size, size).to_rgba8();

        let r = size as f32 / 2.0;
        let cx = r;
        let cy = r;
        for (px, py, pixel) in square.enumerate_pixels_mut() {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy > r * r {
                *pixel = Rgba([0u8, 0, 0, 0]);
            }
        }
        DynamicImage::ImageRgba8(square)
    } else {
        dyn_img
    };

    Ok(picker.new_resize_protocol(img))
}

fn synthesize_cover() -> RgbaImage {
    let (w, h) = (64u32, 64u32);
    let mut buf = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let t = (x + y) as f32 / (w + h) as f32;
            buf.put_pixel(
                x,
                y,
                Rgba([
                    (30.0 + 200.0 * t) as u8,
                    (120.0 + 100.0 * (1.0 - t)) as u8,
                    (200.0 + 55.0 * t) as u8,
                    255,
                ]),
            );
        }
    }
    buf
}

fn fmt_secs(total_secs: f64) -> String {
    let m = (total_secs as u64) / 60;
    let s = (total_secs as u64) % 60;
    format!("{m}:{s:02}")
}

fn draw(f: &mut Frame, app: &mut App) {
    let area = Layout::vertical([Constraint::Min(0), Constraint::Length(5)]).split(f.area())[1];

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(ratatui::style::Color::Rgb(60, 60, 70)))
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::horizontal([
        Constraint::Length(14),
        Constraint::Min(18),
        Constraint::Length(16),
        Constraint::Length(3),
    ])
    .split(inner);

    let image = StatefulImage::new().resize(Resize::Fit(None));
    f.render_stateful_widget(image, cols[0], &mut app.cover);

    let mid = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(cols[1]);

    let info = vec![Line::from(vec![
        Span::styled(
            "\u{f001} ",
            Style::default().fg(ratatui::style::Color::Rgb(194, 12, 12)),
        ),
        Span::styled(
            SONG_NAME,
            Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{SONG_SINGER} ◈  {SONG_ALBUM}"),
            Style::default().fg(ratatui::style::Color::Rgb(120, 120, 130)),
        ),
    ])];
    f.render_widget(Paragraph::new(info), mid[0]);

    let play_icon = if app.paused || !app.playing {
        "\u{f04b}"
    } else {
        "\u{f04c}"
    };
    let controls = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "\u{f049}",
            Style::default().fg(ratatui::style::Color::Rgb(120, 120, 130)),
        ),
        Span::raw("    "),
        Span::styled(
            play_icon,
            Style::default()
                .fg(ratatui::style::Color::Rgb(194, 12, 12))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled(
            "\u{f04e}",
            Style::default().fg(ratatui::style::Color::Rgb(120, 120, 130)),
        ),
        Span::raw("  "),
    ])
    .alignment(Alignment::Center);
    f.render_widget(Paragraph::new(controls), mid[1]);

    let dur_secs = 217.0;
    let cur_secs = app.progress * dur_secs;
    let time_str = format!("{} / {}", fmt_secs(cur_secs), fmt_secs(dur_secs));
    let gauge = LineGauge::default()
        .filled_symbol("━")
        .unfilled_symbol("─")
        .filled_style(Style::default().fg(ratatui::style::Color::Rgb(194, 12, 12)))
        .unfilled_style(Style::default().fg(ratatui::style::Color::Rgb(120, 120, 130)))
        .ratio(app.progress)
        .label(Span::styled(
            time_str,
            Style::default().fg(ratatui::style::Color::White),
        ));
    f.render_widget(gauge, mid[2]);

    let vol_label = format!("\u{f028}");
    let vol_gauge = LineGauge::default()
        .filled_symbol("━")
        .unfilled_symbol("─")
        .filled_style(Style::default().fg(ratatui::style::Color::Rgb(120, 120, 130)))
        .unfilled_style(Style::default().fg(ratatui::style::Color::Rgb(60, 60, 70)))
        .ratio(app.volume)
        .label(Span::styled(
            vol_label,
            Style::default().fg(ratatui::style::Color::Rgb(150, 150, 160)),
        ));
    f.render_widget(vol_gauge, cols[2]);

    f.render_widget(
        Paragraph::new(Span::styled(
            "\u{f01e}",
            Style::default().fg(ratatui::style::Color::Rgb(194, 12, 12)),
        ))
        .alignment(Alignment::Center),
        cols[3],
    );
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cover_path = std::env::args().nth(1);

    enable_raw_mode()?;
    let mut out: Stdout = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let picker = Picker::from_query_stdio()?;
    let mut app = App::new(&picker, cover_path.as_deref())?;

    let result = run(&mut terminal, &mut app, &picker, cover_path.as_deref());

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    picker: &Picker,
    cover_path: Option<&str>,
) -> color_eyre::Result<()> {
    let tick_rate = Duration::from_millis(1000 / 30);
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char(' ') => app.paused = !app.paused,
                    KeyCode::Char('s') => app.seeking = !app.seeking,
                    KeyCode::Char('c') => {
                        app.circle = !app.circle;
                        app.cover = load_cover(&picker, cover_path, app.circle)?;
                    }
                    KeyCode::Char('-') => {
                        app.volume = (app.volume - 0.05).clamp(0.0, 1.0);
                    }
                    KeyCode::Char('=') | KeyCode::Char('+') => {
                        app.volume = (app.volume + 0.05).clamp(0.0, 1.0);
                    }
                    _ => {}
                }
            }
        }
        app.tick();
    }
    Ok(())
}
