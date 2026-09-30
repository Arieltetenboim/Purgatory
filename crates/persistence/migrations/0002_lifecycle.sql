-- Phase 12B lifecycle. Applied after 0001_foundation.sql. Do not edit 0001.
-- Character leases and channel generations are different identifiers.
-- Ground rows record the channel generation that owns the drop so one
-- process cannot retire another live channel.

CREATE TABLE character_leases (
    owner_login text PRIMARY KEY REFERENCES dev_users (login),
    character_id bytea NOT NULL REFERENCES characters (character_id),
    generation bigint NOT NULL,
    expires_at timestamptz NOT NULL,
    CONSTRAINT character_leases_generation_positive CHECK (generation > 0),
    CONSTRAINT character_leases_id_width CHECK (octet_length(character_id) = 8)
);

CREATE TABLE channel_generations (
    channel_id bigint PRIMARY KEY,
    generation bigint NOT NULL,
    expires_at timestamptz NOT NULL,
    CONSTRAINT channel_generations_id_nonnegative CHECK (channel_id >= 0),
    CONSTRAINT channel_generations_generation_positive CHECK (generation > 0)
);

ALTER TABLE item_instances
    ADD COLUMN ground_channel_id bigint,
    ADD COLUMN ground_generation bigint;

ALTER TABLE item_instances
    ADD CONSTRAINT item_instances_ground_scope CHECK (
        (
            state = 'live'
            AND location_kind = 'ground'
            AND (
                (ground_channel_id IS NULL AND ground_generation IS NULL)
                OR (
                    ground_channel_id IS NOT NULL
                    AND ground_channel_id >= 0
                    AND ground_generation IS NOT NULL
                    AND ground_generation > 0
                )
            )
        )
        OR (
            (state IS DISTINCT FROM 'live' OR location_kind IS DISTINCT FROM 'ground')
            AND ground_channel_id IS NULL
            AND ground_generation IS NULL
        )
    );

CREATE INDEX item_instances_ground_channel
    ON item_instances (ground_channel_id, ground_generation)
    WHERE state = 'live' AND location_kind = 'ground';
