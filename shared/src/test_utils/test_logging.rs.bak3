use env_logger::Builder;
use log::LevelFilter;
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test logging with appropriate log level
///
/// This ensures logs are only shown when tests fail or when LOG_LEVEL env var is set.
/// Usage: Call init_test_logging() at the beginning of each test file.
pub fn init_test_logging() {
    INIT.call_once(|| {
        let mut builder = Builder::from_default_env();

        // Default to Error level in tests to reduce noise
        // Users can override with LOG_LEVEL env var if they want more details
        let level_filter = if std::env::var("LOG_LEVEL").is_ok() {
            match std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "error".to_string())
                .as_str()
            {
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                "trace" => LevelFilter::Trace,
                _ => LevelFilter::Error,
            }
        } else {
            LevelFilter::Error
        };

        builder.filter_level(level_filter).is_test(true).init();
    });
}
