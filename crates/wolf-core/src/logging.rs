//! Process-global tracing setup for the app.

/// Installs the process-global stderr `tracing` subscriber.
///
/// Called once per [`wolf_ffi_start`](../../wolf_ffi/fn.wolf_ffi_start.html): the
/// FIRST call installs the subscriber and wins; every subsequent call sees
/// `try_init` return `Err` (a global default is already set), and that error is
/// discarded. The function is therefore **idempotent** and **never panics**,
/// regardless of how many projects/windows start.
///
/// Verbosity is gated by the `WOLF_LOG` environment variable using standard
/// [`EnvFilter`](tracing_subscriber::EnvFilter) directive syntax
/// (e.g. `WOLF_LOG=wolf_core=debug`). When `WOLF_LOG` is unset or unparseable the
/// default level is `warn`, so the RPC-timeout `warn!` is visible on stderr with
/// no configuration while normal runs stay quiet. Output goes to stderr only,
/// with ANSI colour disabled, targets on, and thread ids on.
pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("WOLF_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Installing the tracing subscriber twice must be a harmless no-op.
    ///
    /// [`init_tracing`] installs a process-global default subscriber via
    /// `try_init`, discarding its `Result`. The FIRST call wins and installs the
    /// subscriber; a SECOND call (e.g. a second `wolf_ffi_start` for another
    /// project) sees `try_init` return `Err` and must simply return WITHOUT
    /// panicking. This is the whole idempotency contract — there is exactly one
    /// test here because installing a global default is process-wide and cannot be
    /// meaningfully repeated across separate tests in the same binary.
    #[test]
    fn init_tracing_is_idempotent() {
        init_tracing();
        // The second call must not panic: try_init's Err is swallowed internally.
        init_tracing();
    }
}
