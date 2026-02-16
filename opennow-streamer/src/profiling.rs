//! Profiling Module
//!
//! Optional Tracy profiler integration for performance analysis.
//! Enable with: cargo build --release --features tracy
//!
//! Usage:
//! 1. Build with tracy feature: cargo build --release --features tracy
//! 2. Download Tracy profiler from https://github.com/wolfpld/tracy/releases
//! 3. Run Tracy profiler and click "Connect"
//! 4. Run the application - Tracy will capture real-time profiling data

/// Initialize the profiling system
/// Returns true if logging was initialized (caller should NOT call env_logger::init)
/// Returns false if caller should initialize logging themselves
pub fn init() -> bool {
    #[cfg(feature = "tracy")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        // Create Tracy layer for profiling
        let tracy_layer = tracing_tracy::TracyLayer::default();

        // Create fmt layer for console logging (replaces env_logger when tracy is enabled)
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_level(true);

        // Create env filter (reads RUST_LOG, defaults to info)
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        // Set up tracing subscriber with both Tracy and console output
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(tracy_layer)
            .init();

        // Also set up log -> tracing bridge so log macros (info!, warn!, etc.) work
        tracing_log::LogTracer::init().ok();

        return true; // Logging initialized, caller should NOT call env_logger
    }

    #[cfg(not(feature = "tracy"))]
    {
        return false; // Caller should initialize logging
    }
}

/// Profile a scope with a given name
/// This macro creates a tracing span that Tracy can visualize
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        #[cfg(feature = "tracy")]
        let _span = tracing::info_span!($name).entered();
    };
}

/// Mark a frame boundary for Tracy's frame view
/// Call this once per frame in the main render loop
#[inline]
pub fn frame_mark() {
    #[cfg(feature = "tracy")]
    {
        tracing_tracy::client::frame_mark();
    }
}
