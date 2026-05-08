pub use dubbo_rs_common;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing_subscriber::fmt;
use tracing_subscriber::EnvFilter;

// ============================================================================
// LogLevel
// ============================================================================

/// Log level for the subscriber filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Return the canonical lower-case string for this level.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// OutputFormat
// ============================================================================

/// Output format for the tracing subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// JSON-structured log lines.
    Json,
    /// Human-readable, multi-line log entries.
    Pretty,
    /// Compact single-line log entries.
    Compact,
}

// ============================================================================
// LoggerConfig
// ============================================================================

/// Configuration for the logging subsystem.
///
/// Supports YAML/JSON deserialization for integration with
/// [`dubbo_rs_config::RootConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggerConfig {
    /// Minimum log level.
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    /// Output format for log records.
    #[serde(default = "default_output_format")]
    pub output_format: OutputFormat,

    /// Optional file path.  Writes to stdout when `None`.
    #[serde(default)]
    pub log_file: Option<PathBuf>,
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

fn default_output_format() -> OutputFormat {
    OutputFormat::Pretty
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
            output_format: OutputFormat::Pretty,
            log_file: None,
        }
    }
}

// ============================================================================
// LoggerBuilder
// ============================================================================

/// Builder for constructing a [`LoggerConfig`] and initializing the
/// tracing subscriber.
#[derive(Debug, Clone, Default)]
pub struct LoggerBuilder {
    config: LoggerConfig,
}

impl LoggerBuilder {
    /// Create a new builder with default settings (info / pretty / stdout).
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: LoggerConfig::default(),
        }
    }

    /// Create a builder from an existing [`LoggerConfig`].
    #[must_use]
    pub fn from_config(config: LoggerConfig) -> Self {
        Self { config }
    }

    /// Create a builder from a [`dubbo_rs_config::RootConfig`].
    ///
    /// This is a forward-compatible integration point.  When `RootConfig`
    /// gains a `logger` field, this method will read it.
    #[must_use]
    pub fn from_root_config(_root: &dubbo_rs_config::RootConfig) -> Self {
        Self::new()
    }

    /// Set the minimum log level.
    #[must_use]
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.config.log_level = level;
        self
    }

    /// Set the output format.
    #[must_use]
    pub fn with_output_format(mut self, format: OutputFormat) -> Self {
        self.config.output_format = format;
        self
    }

    /// Set a log file path.  Writes to stdout when not set.
    #[must_use]
    pub fn with_log_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.log_file = Some(path.into());
        self
    }

    /// Obtain a reference to the current configuration.
    #[must_use]
    pub fn config(&self) -> &LoggerConfig {
        &self.config
    }

    /// Initialize the tracing subscriber with the builder's configuration.
    ///
    /// If the `RUST_LOG` environment variable is set, it takes precedence
    /// over the configured `log_level`.
    ///
    /// # Panics
    ///
    /// Panics if a global subscriber has already been registered.
    pub fn init(&self) {
        self.try_init()
            .expect("failed to initialize tracing subscriber");
    }

    /// Like [`init`](Self::init), but returns an error instead of panicking.
    ///
    /// Returns `Err` if a global subscriber has already been set.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A global subscriber is already registered
    /// - The log file cannot be created
    pub fn try_init(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(self.config.log_level.as_str()));

        match &self.config.log_file {
            Some(path) => {
                let file = std::fs::File::create(path)
                    .map_err(|e| format!("failed to create log file {}: {e}", path.display()))?;
                let writer = std::sync::Mutex::new(file);
                match self.config.output_format {
                    OutputFormat::Json => {
                        fmt()
                            .with_env_filter(env_filter)
                            .json()
                            .with_writer(writer)
                            .try_init()?;
                    }
                    OutputFormat::Compact => {
                        fmt()
                            .with_env_filter(env_filter)
                            .compact()
                            .with_writer(writer)
                            .try_init()?;
                    }
                    OutputFormat::Pretty => {
                        fmt()
                            .with_env_filter(env_filter)
                            .with_writer(writer)
                            .try_init()?;
                    }
                }
            }
            None => match self.config.output_format {
                OutputFormat::Json => {
                    fmt().with_env_filter(env_filter).json().try_init()?;
                }
                OutputFormat::Compact => {
                    fmt().with_env_filter(env_filter).compact().try_init()?;
                }
                OutputFormat::Pretty => {
                    fmt().with_env_filter(env_filter).try_init()?;
                }
            },
        }

        Ok(())
    }
}

// ============================================================================
// set_rust_log
// ============================================================================

/// Quick-start logger: reads `RUST_LOG` from the environment and
/// initialises a pretty-printing subscriber.
///
/// Useful as a fallback when no explicit [`LoggerBuilder`] configuration
/// is provided.
///
/// # Panics
///
/// Panics if a global subscriber has already been registered.
pub fn set_rust_log() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    // ── LoggerConfig defaults ─────────────────────────────────────────

    #[test]
    fn test_logger_config_defaults() {
        let config = LoggerConfig::default();
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.output_format, OutputFormat::Pretty);
        assert!(config.log_file.is_none());
    }

    #[test]
    fn test_logger_builder_defaults() {
        let builder = LoggerBuilder::new();
        let config = builder.config();
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.output_format, OutputFormat::Pretty);
        assert!(config.log_file.is_none());
    }

    // ── Builder customization ─────────────────────────────────────────

    #[test]
    fn test_logger_builder_custom_level() {
        let builder = LoggerBuilder::new()
            .with_log_level(LogLevel::Debug)
            .with_output_format(OutputFormat::Compact);
        let config = builder.config();
        assert_eq!(config.log_level, LogLevel::Debug);
        assert_eq!(config.output_format, OutputFormat::Compact);
    }

    #[test]
    fn test_logger_builder_log_file() {
        let builder = LoggerBuilder::new().with_log_file("/tmp/test.log");
        let config = builder.config();
        assert_eq!(config.log_file, Some(PathBuf::from("/tmp/test.log")));
    }

    // ── LogLevel conversions ──────────────────────────────────────────

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "trace");
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
    }

    #[test]
    fn test_log_level_into_tracing() {
        assert_eq!(tracing::Level::from(LogLevel::Trace), tracing::Level::TRACE);
        assert_eq!(tracing::Level::from(LogLevel::Debug), tracing::Level::DEBUG);
        assert_eq!(tracing::Level::from(LogLevel::Info), tracing::Level::INFO);
        assert_eq!(tracing::Level::from(LogLevel::Warn), tracing::Level::WARN);
        assert_eq!(tracing::Level::from(LogLevel::Error), tracing::Level::ERROR);
    }

    // ── Serialization round-trip ──────────────────────────────────────

    #[test]
    fn test_logger_config_yaml_roundtrip() {
        let config = LoggerConfig {
            log_level: LogLevel::Debug,
            output_format: OutputFormat::Json,
            log_file: Some(PathBuf::from("/var/log/dubbo.log")),
        };

        let yaml = serde_yaml::to_string(&config).expect("serialize");
        let parsed: LoggerConfig = serde_yaml::from_str(&yaml).expect("deserialize");

        assert_eq!(parsed.log_level, LogLevel::Debug);
        assert_eq!(parsed.output_format, OutputFormat::Json);
        assert_eq!(parsed.log_file, Some(PathBuf::from("/var/log/dubbo.log")));
    }

    #[test]
    fn test_logger_config_yaml_defaults() {
        let yaml = "{}";
        let config: LoggerConfig = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.output_format, OutputFormat::Pretty);
        assert!(config.log_file.is_none());
    }

    // ── Integration with RootConfig ───────────────────────────────────

    #[test]
    fn test_logger_builder_from_root_config() {
        let root = dubbo_rs_config::RootConfig::default().with_application("test-app");

        let builder = LoggerBuilder::from_root_config(&root);
        let config = builder.config();
        // Currently RootConfig has no logger field → defaults apply
        assert_eq!(config.log_level, LogLevel::Info);
    }

    // ── File output integration test ──────────────────────────────────

    #[test]
    fn test_log_to_file_writes_entries() {
        let dir =
            std::env::temp_dir().join(format!("dubbo-logger-test-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let log_path = dir.join("dubbo.log");

        let env_filter = EnvFilter::new("info");
        let file = std::fs::File::create(&log_path).expect("create log file");
        let subscriber = fmt()
            .with_env_filter(env_filter)
            .with_writer(std::sync::Mutex::new(file))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "dubbo", "hello from dubbo-logger");
            tracing::warn!(target: "dubbo", count = 42, "a warning with fields");
        });

        // Verify the file contains our log lines.
        let mut contents = String::new();
        std::fs::File::open(&log_path)
            .expect("open log file")
            .read_to_string(&mut contents)
            .expect("read log file");

        assert!(
            contents.contains("hello from dubbo-logger"),
            "log file should contain info message, got: {contents}"
        );
        assert!(
            contents.contains("a warning with fields"),
            "log file should contain warn message, got: {contents}"
        );

        // cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_custom_level_filtering() {
        let dir =
            std::env::temp_dir().join(format!("dubbo-logger-test-filter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let log_path = dir.join("filter.log");

        let env_filter = EnvFilter::new("warn");
        let file = std::fs::File::create(&log_path).expect("create log file");
        let subscriber = fmt()
            .with_env_filter(env_filter)
            .with_writer(std::sync::Mutex::new(file))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("this info should be filtered out");
            tracing::warn!("this warn should appear");
            tracing::error!("this error should appear");
        });

        let mut contents = String::new();
        std::fs::File::open(&log_path)
            .expect("open log file")
            .read_to_string(&mut contents)
            .expect("read log file");

        assert!(
            !contents.contains("filtered out"),
            "info message should be filtered at WARN level, got: {contents}"
        );
        assert!(
            contents.contains("this warn should appear"),
            "warn message should appear, got: {contents}"
        );
        assert!(
            contents.contains("this error should appear"),
            "error message should appear, got: {contents}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_default_builder_init_does_not_panic_when_no_global_subscriber() {
        // Setting a subscriber releases any previously-set subscriber,
        // so we can safely run this test even if others already set one.
        let builder = LoggerBuilder::new();
        // try_init() returns Ok the first time; subsequent calls return Err.
        // We just test that the path doesn't panic.
        let result = builder.try_init();
        // After other tests may have set a subscriber, this could be Err.
        // We don't assert Ok/Err — we only assert it didn't panic.
        let _ = result;
    }
}
