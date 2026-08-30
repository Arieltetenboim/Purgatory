//! Logical world membership. Independent of X/Y transform.

/// Authored or runtime map identity. Not a runtime entity id.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct MapId(u32);

impl MapId {
    pub const DEV: Self = Self(1);

    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for MapId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Channel within a map. Default `0` is the development channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ChannelId(u32);

impl ChannelId {
    pub const DEFAULT: Self = Self(0);

    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Instance within a map+channel. Default `0` is the development instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct InstanceId(u32);

impl InstanceId {
    pub const DEFAULT: Self = Self(0);

    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Logical world membership (`MapId` + `ChannelId` + `InstanceId`).
/// Independent of transform and of identity / social scope.
///
/// Entities in incompatible addresses are not automatically mutually visible.
/// That is a **WorldAddress boundary**, not a social-identity boundary:
/// Channel membership isolates replication, AOI, and world-bound interaction.
/// Future whisper / friends / party / guild / presence must not be keyed to
/// Channel (those systems are not implemented yet).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct WorldAddress {
    pub map: MapId,
    pub channel: ChannelId,
    pub instance: InstanceId,
}

impl WorldAddress {
    /// Development FOOTNOTE / Phase 5 stage membership.
    pub const DEV: Self = Self {
        map: MapId::DEV,
        channel: ChannelId::DEFAULT,
        instance: InstanceId::DEFAULT,
    };

    #[must_use]
    pub const fn new(map: MapId, channel: ChannelId, instance: InstanceId) -> Self {
        Self {
            map,
            channel,
            instance,
        }
    }

    /// Compatible for visibility/query: same map, channel, and instance.
    #[must_use]
    pub const fn compatible_with(self, other: Self) -> bool {
        self.map.0 == other.map.0
            && self.channel.0 == other.channel.0
            && self.instance.0 == other.instance.0
    }
}

impl std::fmt::Display for WorldAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "map={} ch={} inst={}",
            self.map, self.channel, self.instance
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_address_is_compatible() {
        assert!(WorldAddress::DEV.compatible_with(WorldAddress::DEV));
        assert_eq!(WorldAddress::DEV, WorldAddress::DEV);
    }

    #[test]
    fn different_map_is_incompatible() {
        let other = WorldAddress::new(MapId::from_raw(2), ChannelId::DEFAULT, InstanceId::DEFAULT);
        assert!(!WorldAddress::DEV.compatible_with(other));
        assert_ne!(WorldAddress::DEV, other);
    }

    #[test]
    fn different_channel_is_incompatible() {
        let other = WorldAddress::new(MapId::DEV, ChannelId::from_raw(1), InstanceId::DEFAULT);
        assert!(!WorldAddress::DEV.compatible_with(other));
    }

    #[test]
    fn different_instance_is_incompatible() {
        let other = WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(9));
        assert!(!WorldAddress::DEV.compatible_with(other));
    }

    #[test]
    fn address_is_independent_of_coordinates() {
        let a = WorldAddress::DEV;
        let b = WorldAddress::DEV;
        assert_eq!(a, b);
        assert_eq!(format!("{a}"), "map=1 ch=0 inst=0");
    }
}
