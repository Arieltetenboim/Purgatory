//! Logical content domains. Shared data may load on the client; server-only must not.

/// Who may consume a definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentDomain {
    Shared,
    ServerOnly,
}

impl ContentDomain {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::ServerOnly => "server",
        }
    }
}
