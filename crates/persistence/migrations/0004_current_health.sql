-- Phase 12C current health. Applied after 0003_item_id_reservations.sql.
-- Do not edit 0001, 0002, or 0003.
--
-- Existing character rows have no stored HP. current_health stays NULL, and
-- load treats NULL as that character's full maximum. The first save that
-- records a changed value writes an explicit number. health_revision starts
-- at 0. Health is updated only when the snapshot's health_revision is newer.
-- Map and equipment writes do not copy an older health value over a newer one.

ALTER TABLE characters
    ADD COLUMN current_health double precision,
    ADD COLUMN health_revision bigint NOT NULL DEFAULT 0;

ALTER TABLE characters
    ADD CONSTRAINT characters_current_health_nonnegative
        CHECK (current_health IS NULL OR current_health >= 0),
    ADD CONSTRAINT characters_health_revision_nonnegative
        CHECK (health_revision >= 0);
