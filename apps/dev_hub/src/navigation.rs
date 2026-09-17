//! Hub navigation. Placeholders reserve space; they are not fake tools.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubPage {
    Dashboard,
    RuntimeServer,
    RuntimeClients,
    LaunchProfiles,
    Validation,
    Performance,
    Phase7Stats,
    World,
    Content,
    Logs,
    Doctor,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind { Live, Placeholder }

impl HubPage {
    pub const ALL: [HubPage; 12] = [
        Self::Dashboard,
        Self::RuntimeServer,
        Self::RuntimeClients,
        Self::LaunchProfiles,
        Self::World,
        Self::Content,
        Self::Validation,
        Self::Performance,
        Self::Phase7Stats,
        Self::Logs,
        Self::Doctor,
        Self::Settings,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::RuntimeServer => "Server",
            Self::RuntimeClients => "Clients",
            Self::LaunchProfiles => "Launch Profiles",
            Self::Validation => "Validation",
            Self::Performance => "Performance",
            Self::Phase7Stats => "Phase 7 Stats",
            Self::World => "World",
            Self::Content => "Content",
            Self::Logs => "Logs",
            Self::Doctor => "Environment Doctor",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Dashboard => "⌂",
            Self::RuntimeServer => "▣",
            Self::RuntimeClients => "▤",
            Self::LaunchProfiles => "▶",
            Self::Validation => "⛨",
            Self::Performance => "▥",
            Self::Phase7Stats => "▦",
            Self::World => "◈",
            Self::Content => "◇",
            Self::Logs => "≡",
            Self::Doctor => "+",
            Self::Settings => "⚙",
        }
    }

    #[must_use]
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::Dashboard => Some("Overview"),
            Self::RuntimeServer | Self::RuntimeClients | Self::LaunchProfiles => Some("Runtime"),
            Self::Validation | Self::Performance | Self::Phase7Stats => Some("Testing"),
            Self::World | Self::Content => Some("Authoring"),
            Self::Logs | Self::Doctor | Self::Settings => Some("System"),
        }
    }

    #[must_use]
    pub fn kind(self) -> PageKind {
        match self {
            Self::World => PageKind::Placeholder,
            _ => PageKind::Live,
        }
    }

    #[must_use]
    pub fn placeholder_blurb(self) -> &'static str {
        match self {
            Self::World => "Map/world editing is reserved for a future Hub module.",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_world_remains_placeholder() {
        let placeholders: Vec<_> = HubPage::ALL.iter().copied().filter(|p| p.kind() == PageKind::Placeholder).collect();
        assert_eq!(placeholders, vec![HubPage::World]);
    }
}
