//! Hub navigation. Placeholders reserve space; they are not fake tools.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubPage {
    Dashboard,
    RuntimeServer,
    RuntimeClients,
    Validation,
    Performance,
    Phase7Stats,
    World,
    Content,
    Logs,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Live,
    Placeholder,
}

impl HubPage {
    pub const ALL: [HubPage; 10] = [
        Self::Dashboard,
        Self::RuntimeServer,
        Self::RuntimeClients,
        Self::Validation,
        Self::Performance,
        Self::Phase7Stats,
        Self::World,
        Self::Content,
        Self::Logs,
        Self::Settings,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::RuntimeServer => "Server",
            Self::RuntimeClients => "Clients",
            Self::Validation => "Validation",
            Self::Performance => "Performance",
            Self::Phase7Stats => "Phase 7 Stats",
            Self::World => "World",
            Self::Content => "Content",
            Self::Logs => "Logs",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Dashboard => "⌂",
            Self::RuntimeServer => "▣",
            Self::RuntimeClients => "▤",
            Self::Validation => "⛨",
            Self::Performance => "▥",
            Self::Phase7Stats => "▦",
            Self::World => "◈",
            Self::Content => "◇",
            Self::Logs => "≡",
            Self::Settings => "⚙",
        }
    }

    #[must_use]
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::Dashboard => Some("Overview"),
            Self::RuntimeServer | Self::RuntimeClients => Some("Runtime"),
            Self::Validation | Self::Performance | Self::Phase7Stats => Some("Testing"),
            Self::World | Self::Content => Some("Authoring"),
            Self::Logs | Self::Settings => Some("System"),
        }
    }

    #[must_use]
    pub fn kind(self) -> PageKind {
        match self {
            Self::Dashboard
            | Self::RuntimeServer
            | Self::RuntimeClients
            | Self::Logs
            | Self::Validation
            | Self::Performance
            | Self::Phase7Stats
            | Self::Content
            | Self::Settings => PageKind::Live,
            _ => PageKind::Placeholder,
        }
    }

    #[must_use]
    pub fn placeholder_blurb(self) -> &'static str {
        match self {
            Self::RuntimeClients => "Open +1/+2/+3 clients after Server Ready. F6 queues one.",
            Self::Validation => "Runtime Validation is live. Use this page to start a harness.",
            Self::Performance => "Load/soak launcher is live on this page.",
            Self::Phase7Stats => {
                "Frozen Phase 7.8 gate summary from artifacts. HARNESS WARN ≠ SERVER WARN."
            }
            Self::World => "Map/world editing is reserved for a future Hub module.",
            Self::Content => {
                "Launch the standalone Animation Lab. Other content editors are not in A7.0."
            }
            Self::Settings => "Build profile, log level, quality gate, rebuild, Kill All.",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_pages_cover_launcher_parity() {
        let live: Vec<_> = HubPage::ALL
            .iter()
            .copied()
            .filter(|p| p.kind() == PageKind::Live)
            .collect();
        assert_eq!(
            live,
            vec![
                HubPage::Dashboard,
                HubPage::RuntimeServer,
                HubPage::RuntimeClients,
                HubPage::Validation,
                HubPage::Performance,
                HubPage::Phase7Stats,
                HubPage::Content,
                HubPage::Logs,
                HubPage::Settings,
            ]
        );
    }
}
