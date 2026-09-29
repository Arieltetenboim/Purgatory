-- Phase 12A durable foundation.
-- One database owns every channel. Ground rows are temporary ownership, not a
-- restored map snapshot. Runtime EntityId, ConnectionId, and session state are
-- not stored. Dialogue heard uses an authored beat id, never a beat index.

CREATE TABLE schema_migrations (
    version integer PRIMARY KEY,
    body text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE durable_meta (
    key text PRIMARY KEY,
    value text NOT NULL
);

CREATE TABLE dev_users (
    login text PRIMARY KEY,
    CONSTRAINT dev_users_login_length CHECK (char_length(login) BETWEEN 2 AND 32)
);

CREATE TABLE characters (
    character_id bytea PRIMARY KEY,
    owner_login text NOT NULL REFERENCES dev_users (login),
    display_name text NOT NULL,
    name_key text NOT NULL,
    roster_position integer NOT NULL,
    persistence_revision bigint NOT NULL,
    restore_map_authored text NOT NULL,
    restore_point_id text NOT NULL,
    restore_checkpoint_id text,
    instance_exit_reason text,
    CONSTRAINT characters_id_width CHECK (octet_length(character_id) = 8),
    CONSTRAINT characters_id_nonzero CHECK (character_id <> '\x0000000000000000'::bytea),
    CONSTRAINT characters_revision_positive CHECK (persistence_revision > 0),
    CONSTRAINT characters_roster_position CHECK (roster_position >= 0 AND roster_position < 3),
    CONSTRAINT characters_restore_present CHECK (
        char_length(restore_map_authored) > 0 AND char_length(restore_point_id) > 0
    ),
    CONSTRAINT characters_owner_roster UNIQUE (owner_login, roster_position),
    CONSTRAINT characters_name_key_unique UNIQUE (name_key)
);

CREATE TABLE item_instances (
    item_instance_id bytea PRIMARY KEY,
    definition_content_id integer NOT NULL,
    quantity integer NOT NULL,
    state text NOT NULL,
    owner_character_id bytea REFERENCES characters (character_id),
    location_kind text,
    inventory_slot smallint,
    equipment_slot text,
    CONSTRAINT item_instances_id_width CHECK (octet_length(item_instance_id) = 8),
    CONSTRAINT item_instances_id_nonzero CHECK (item_instance_id <> '\x0000000000000000'::bytea),
    CONSTRAINT item_instances_quantity_positive CHECK (quantity > 0),
    CONSTRAINT item_instances_state CHECK (state IN ('live', 'retired')),
    CONSTRAINT item_instances_owner_exclusive CHECK (
        (
            state = 'retired'
            AND owner_character_id IS NULL
            AND location_kind IS NULL
            AND inventory_slot IS NULL
            AND equipment_slot IS NULL
        )
        OR (
            state = 'live'
            AND location_kind = 'ground'
            AND owner_character_id IS NULL
            AND inventory_slot IS NULL
            AND equipment_slot IS NULL
        )
        OR (
            state = 'live'
            AND location_kind = 'inventory'
            AND owner_character_id IS NOT NULL
            AND inventory_slot IS NOT NULL
            AND inventory_slot >= 0
            AND inventory_slot < 20
            AND equipment_slot IS NULL
        )
        OR (
            state = 'live'
            AND location_kind = 'equipped'
            AND owner_character_id IS NOT NULL
            AND equipment_slot IN ('headwear', 'bodywear', 'pants', 'gloves', 'boots', 'weapon')
            AND inventory_slot IS NULL
        )
    )
);

-- One live item id is the primary key. These indexes keep one occupant per slot.
CREATE UNIQUE INDEX item_live_inventory_slot
    ON item_instances (owner_character_id, inventory_slot)
    WHERE state = 'live' AND location_kind = 'inventory';

CREATE UNIQUE INDEX item_live_equipment_slot
    ON item_instances (owner_character_id, equipment_slot)
    WHERE state = 'live' AND location_kind = 'equipped';

CREATE INDEX item_instances_live_ground
    ON item_instances (item_instance_id)
    WHERE state = 'live' AND location_kind = 'ground';

CREATE TABLE character_facts (
    character_id bytea NOT NULL REFERENCES characters (character_id),
    fact_key text NOT NULL,
    value boolean NOT NULL,
    PRIMARY KEY (character_id, fact_key),
    CONSTRAINT character_facts_key_length CHECK (char_length(fact_key) BETWEEN 1 AND 64)
);

CREATE TABLE character_npcs_met (
    character_id bytea NOT NULL REFERENCES characters (character_id),
    npc_authored text NOT NULL,
    PRIMARY KEY (character_id, npc_authored),
    CONSTRAINT character_npcs_met_key_length CHECK (char_length(npc_authored) BETWEEN 1 AND 80)
);

-- Authored beat id paired with the NPC's numeric ContentId. Not a beat index.
CREATE TABLE character_dialogue_heard (
    character_id bytea NOT NULL REFERENCES characters (character_id),
    npc_content_id integer NOT NULL,
    beat_id text NOT NULL,
    PRIMARY KEY (character_id, npc_content_id, beat_id),
    CONSTRAINT character_dialogue_heard_beat_length CHECK (char_length(beat_id) BETWEEN 1 AND 64)
);

CREATE TABLE character_learned_abilities (
    character_id bytea NOT NULL REFERENCES characters (character_id),
    ability_content_id integer NOT NULL,
    PRIMARY KEY (character_id, ability_content_id)
);

CREATE TABLE durable_commands (
    command_key text PRIMARY KEY,
    request_json text NOT NULL,
    result_json text NOT NULL,
    CONSTRAINT durable_commands_key_length CHECK (char_length(command_key) BETWEEN 1 AND 128)
);
