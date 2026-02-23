use std::{fs, path::Path};

use anyhow::Context;
use chrono::Utc;
use tracing::{info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub struct TelemetryGuard {
    pub session_id: Uuid,
    _file_guard: WorkerGuard,
}

pub fn init_tracing(log_dir: impl AsRef<Path>) -> anyhow::Result<TelemetryGuard> {
    init_tracing_with_options(
        log_dir,
        "voltlane",
        "info,voltlane_core=trace,voltlane_tauri=trace,tauri_plugin_log=info",
    )
}

pub fn init_tracing_with_file_prefix(
    log_dir: impl AsRef<Path>,
    file_prefix: &str,
) -> anyhow::Result<TelemetryGuard> {
    init_tracing_with_options(
        log_dir,
        file_prefix,
        "info,voltlane_core=trace,voltlane_tauri=trace,tauri_plugin_log=info",
    )
}

pub fn init_tracing_with_options(
    log_dir: impl AsRef<Path>,
    file_prefix: &str,
    default_filter: &str,
) -> anyhow::Result<TelemetryGuard> {
    let log_dir = log_dir.as_ref();
    fs::create_dir_all(log_dir)
        .with_context(|| format!("failed to create log directory: {}", log_dir.display()))?;

    let session_id = Uuid::new_v4();
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("{file_prefix}-{timestamp}.log");
    let file_appender = tracing_appender::rolling::never(log_dir, file_name);
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_thread_ids(true)
        .with_target(true);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(file_writer);

    if let Err(error) = tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
    {
        warn!(?error, "global tracing subscriber already initialized");
    } else {
        info!(%session_id, "tracing initialized");
    }

    Ok(TelemetryGuard {
        session_id,
        _file_guard: file_guard,
    })
}
