//! Bounded selection metadata. No persistence or gameplay state crosses this boundary.
use crate::message::{read_bounded_string, read_u64, write_bounded_string};
use crate::{CodecError, ConnectionId};
use purgatory_common::{CharacterId, CharacterName};

pub(crate) const TAG_ENTER_CHARACTER: u8 = 48;
pub(crate) const TAG_ENTER_REJECTED: u8 = 49;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CharacterEnterRejection {
    NotOwned = 1,
    Occupied = 2,
    StorageFailure = 3,
    GameplayEnterFailure = 4,
    InvalidSelection = 5,
}

pub const MAX_CHARACTER_ROSTER: usize = 3;
pub(crate) const TAG_SESSION_READY: u8 = 45;
pub(crate) const TAG_CREATE_CHARACTER: u8 = 46;
pub(crate) const TAG_CREATE_RESULT: u8 = 47;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSummary {
    pub character_id: CharacterId,
    pub display_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontendSessionReady {
    pub connection_id: ConnectionId,
    pub roster: Vec<CharacterSummary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CharacterCreateRejection {
    InvalidName = 1,
    NameTaken = 2,
    RosterFull = 3,
    StorageFailure = 4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreateCharacterResult {
    Created { roster: Vec<CharacterSummary> },
    Rejected(CharacterCreateRejection),
}

pub(crate) fn encode_roster(
    out: &mut Vec<u8>,
    roster: &[CharacterSummary],
) -> Result<(), CodecError> {
    if roster.len() > MAX_CHARACTER_ROSTER {
        return Err(CodecError::InvalidValue);
    }
    out.push(roster.len() as u8);
    for (index, entry) in roster.iter().enumerate() {
        if entry.character_id.raw() == 0
            || roster[..index]
                .iter()
                .any(|old| old.character_id == entry.character_id)
            || CharacterName::parse(&entry.display_name).is_err()
        {
            return Err(CodecError::InvalidValue);
        }
        out.extend_from_slice(&entry.character_id.raw().to_le_bytes());
        write_bounded_string(out, &entry.display_name)?;
    }
    Ok(())
}

pub(crate) fn decode_roster(bytes: &[u8]) -> Result<Vec<CharacterSummary>, CodecError> {
    let (&count, mut rest) = bytes.split_first().ok_or(CodecError::Truncated)?;
    if usize::from(count) > MAX_CHARACTER_ROSTER {
        return Err(CodecError::InvalidValue);
    }
    let mut roster: Vec<CharacterSummary> = Vec::new();
    for _ in 0..count {
        let (id, next) = read_u64(rest)?;
        let (display_name, next) = read_bounded_string(next)?;
        if id == 0
            || roster.iter().any(|entry| entry.character_id.raw() == id)
            || CharacterName::parse(&display_name).is_err()
        {
            return Err(CodecError::InvalidValue);
        }
        roster.push(CharacterSummary {
            character_id: CharacterId::from_raw(id),
            display_name,
        });
        rest = next;
    }
    if !rest.is_empty() {
        return Err(CodecError::TrailingBytes);
    }
    Ok(roster)
}

pub(crate) fn decode_result(bytes: &[u8]) -> Result<CreateCharacterResult, CodecError> {
    let (&tag, rest) = bytes.split_first().ok_or(CodecError::Truncated)?;
    if tag == 0 {
        return Ok(CreateCharacterResult::Created {
            roster: decode_roster(rest)?,
        });
    }
    if !rest.is_empty() {
        return Err(CodecError::TrailingBytes);
    }
    let reason = match tag {
        1 => CharacterCreateRejection::InvalidName,
        2 => CharacterCreateRejection::NameTaken,
        3 => CharacterCreateRejection::RosterFull,
        4 => CharacterCreateRejection::StorageFailure,
        other => return Err(CodecError::UnknownDiscriminant(other)),
    };
    Ok(CreateCharacterResult::Rejected(reason))
}
