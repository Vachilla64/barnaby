use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let appdata = crate::config::get_appdata_dir();
    let log_dir = appdata.join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(&log_dir, "devy.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Keep non-blocking guard alive for process lifetime
    Box::leak(Box::new(guard));

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,devy=debug,tower_http=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .try_init()?;

    tracing::info!("Devy logger initialized at {:?}", log_dir.join("devy.log"));
    Ok(())
}
