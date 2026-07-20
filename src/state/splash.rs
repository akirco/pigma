#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Success,
    Info,
    Warning,
}

#[derive(Debug, Clone)]
pub struct SplashLogEntry {
    pub time: String,
    pub text: String,
    pub level: LogLevel,
}

pub struct SplashState {
    pub progress: f64,
    pub status: String,
    pub logs: Vec<SplashLogEntry>,
    pub boot_complete: bool,
}

impl Default for SplashState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            status: "INITIALIZING SYSTEM...".to_string(),
            logs: Vec::new(),
            boot_complete: false,
        }
    }
}
