//! Diagnostics consumer demand. Read-only assembly runs only when a consumer is active.
//!
//! D1: the in-client overlay is the only consumer. D5 may set `ipc_subscribed`
//! without changing the compose call site to a single `overlay_visible` check.

/// Who wants a diagnostics frame this tick.
///
/// Do not assemble diagnostic-only World walks, inspector rows, string lists,
/// history copies, or probes unless [`Self::is_active`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticsDemand {
    overlay_visible: bool,
    ipc_subscribed: bool,
}

impl DiagnosticsDemand {
    /// Compose demand from every known consumer. Pass `ipc_subscribed = false` until D5.
    #[must_use]
    pub fn compose(overlay_visible: bool, ipc_subscribed: bool) -> Self {
        Self {
            overlay_visible,
            ipc_subscribed,
        }
    }

    #[must_use]
    pub fn is_active(self) -> bool {
        self.overlay_visible || self.ipc_subscribed
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn overlay_visible(self) -> bool {
        self.overlay_visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_when_no_consumer() {
        assert!(!DiagnosticsDemand::compose(false, false).is_active());
    }

    #[test]
    fn overlay_alone_is_active() {
        let demand = DiagnosticsDemand::compose(true, false);
        assert!(demand.is_active());
        assert!(demand.overlay_visible());
    }

    #[test]
    fn ipc_alone_is_active_without_overlay() {
        let demand = DiagnosticsDemand::compose(false, true);
        assert!(demand.is_active());
        assert!(!demand.overlay_visible());
    }
}
