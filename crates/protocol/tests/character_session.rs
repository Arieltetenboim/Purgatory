use purgatory_common::CharacterId;
use purgatory_protocol::*;

fn roster(count: usize) -> Vec<CharacterSummary> {
    (0..count)
        .map(|i| CharacterSummary {
            character_id: CharacterId::from_raw(u64::MAX - i as u64),
            display_name: format!("Hero{i}"),
        })
        .collect()
}

#[test]
fn v32_rosters_preserve_empty_full_order_ids_and_names() {
    assert_eq!(PROTOCOL_VERSION, 32);
    for count in 0..=3 {
        for msg in [
            ServerControl::FrontendSessionReady(FrontendSessionReady {
                connection_id: ConnectionId::from_raw(5),
                roster: roster(count),
            }),
            ServerControl::CreateCharacterResult(CreateCharacterResult::Created {
                roster: roster(count),
            }),
        ] {
            let bytes = encode_server_control(&msg).unwrap();
            assert_eq!(decode_server_control(&bytes).unwrap(), msg);
            for end in 0..bytes.len() {
                assert!(decode_server_control(&bytes[..end]).is_err());
            }
        }
    }
}

#[test]
fn create_carries_only_a_bounded_name_and_typed_rejections() {
    let msg = ClientControl::CreateCharacter {
        name: "Hero".into(),
    };
    let bytes = encode_client_control(&msg).unwrap();
    assert_eq!(bytes, [46, 4, b'H', b'e', b'r', b'o']);
    assert_eq!(decode_client_control(&bytes).unwrap(), msg);
    for reason in [
        CharacterCreateRejection::InvalidName,
        CharacterCreateRejection::NameTaken,
        CharacterCreateRejection::RosterFull,
        CharacterCreateRejection::StorageFailure,
    ] {
        let msg = ServerControl::CreateCharacterResult(CreateCharacterResult::Rejected(reason));
        assert_eq!(
            decode_server_control(&encode_server_control(&msg).unwrap()).unwrap(),
            msg
        );
    }
    let mut oversized = vec![46, 255];
    oversized.extend_from_slice(&[b'a'; 255]);
    assert!(decode_client_control(&oversized).is_err());
    // Domain-invalid but bounded names reach the authoritative validator.
    let invalid = ClientControl::CreateCharacter {
        name: "bad_name".into(),
    };
    assert_eq!(
        decode_client_control(&encode_client_control(&invalid).unwrap()).unwrap(),
        invalid
    );
}

#[test]
fn invalid_roster_boundaries_are_rejected() {
    let message =
        |roster| ServerControl::CreateCharacterResult(CreateCharacterResult::Created { roster });
    assert!(encode_server_control(&message(roster(4))).is_err());
    let mut duplicate = roster(2);
    duplicate[1].character_id = duplicate[0].character_id;
    assert!(encode_server_control(&message(duplicate)).is_err());
    let mut bad_name = roster(1);
    bad_name[0].display_name = "a".repeat(13);
    assert!(encode_server_control(&message(bad_name)).is_err());
    assert!(decode_server_control(&[47, 0, 4]).is_err());
    assert!(decode_server_control(&[47, 9]).is_err());
}
