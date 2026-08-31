//! Hub navigation. Placeholders reserve space; they are not fake tools.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubPage {
    Dashboard,
    RuntimeServer,
    RuntimeClients,
    Validation,
    Performance,
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
    pub const ALL: [HubPage; 9] = [
        Self::Dashboard,
        Self::RuntimeServer,
        Self::RuntimeClients,
        Self::Validation,
        Self::Performance,
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
            Self::World => "World",
            Self::Content => "Content",
            Self::Logs => "Logs",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::Dashboard => None,
            Self::RuntimeServer | Self::RuntimeClients => Some("Runtime"),
            Self::Validation | Self::Performance => Some("Testing"),
            Self::World | Self::Content => Some("Authoring"),
            Self::Logs | Self::Settings => Some("System"),
        }
    }

    #[must_use]
    pub fn kind(self) -> PageKind {
        match self {
            Self::Dashboard | Self::RuntimeServer | Self::Logs | Self::Validation => PageKind::Live,
            _ => PageKind::Placeholder,
        }
    }

    #[must_use]
    pub fn placeholder_blurb(self) -> &'static str {
        match self {
            Self::RuntimeClients => {
                "Client launch controls are later Hub parity. Use PowerShell Developer Tools for now."
            }
            Self::Validation => "Runtime Validation is live. Use this page to start a harness.",
            Self::Performance => {
                "Load/soak and capacity tools are Developer Hub Slice 3. Not started."
            }
            Self::World => "Map/world editing is not in Slice 1. Reserved for a future Hub module.",
            Self::Content => "Content browser/importer/editors are not in Slice 1.",
            Self::Settings => "Settings (persist dir, ports, autostart) are later parity.",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_pages_are_slice1_and_slice2() {
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
                HubPage::Validation,
                HubPage::Logs,
            ]
        );
    }
}
