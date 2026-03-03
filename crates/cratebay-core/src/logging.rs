use crate::store;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub fn init() {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INIT.get_or_init(|| {
        let log_dir = store::log_dir();
        let retention_days = log_retention_days();

        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            eprintln!(
                "CrateBay logging: failed to create log dir {}: {}",
                log_dir.display(),
                e
            );
            return;
        }

        cleanup_old_error_logs(&log_dir, retention_days);

        let error_appender = tracing_appender::rolling::daily(&log_dir, "cratebay-error.log");

        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(env_filter);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(error_appender)
            .with_filter(LevelFilter::WARN);

        let subscriber = tracing_subscriber::registry()
            .with(stdout_layer)
            .with(file_layer);

        if let Err(e) = subscriber.try_init() {
            eprintln!("CrateBay logging: failed to init tracing subscriber: {}", e);
            return;
        }

        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("panic: {}", info);
            default_hook(info);
        }));
    });
}

fn log_retention_days() -> u64 {
    const DEFAULT_DAYS: u64 = 7;

    let Ok(raw) = std::env::var("CRATEBAY_LOG_RETENTION_DAYS") else {
        return DEFAULT_DAYS;
    };

    let Ok(days) = raw.trim().parse::<u64>() else {
        return DEFAULT_DAYS;
    };

    days.clamp(1, 365)
}

fn cleanup_old_error_logs(dir: &Path, retention_days: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let retention = Duration::from_secs(retention_days.saturating_mul(24 * 60 * 60));
    let now = SystemTime::now();
    let cutoff = now.checked_sub(retention).unwrap_or(SystemTime::UNIX_EPOCH);

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !file_name.starts_with("cratebay-error.log.") {
            continue;
        }

        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified >= cutoff {
            continue;
        }

        let _ = std::fs::remove_file(&path);
    }
}
