-- Phase 12C item-id ranges. Applied after 0002_lifecycle.sql. Do not edit 0001 or 0002.
-- A reserved id is usable only by the channel generation that issued it.
-- The counter still advances in the same transaction, so a crash wastes the
-- range and a later generation cannot spend it.

CREATE TABLE item_id_reservations (
    range_start bigint PRIMARY KEY,
    range_end bigint NOT NULL,
    channel_id bigint NOT NULL,
    generation bigint NOT NULL,
    CONSTRAINT item_id_reservations_end_after_start CHECK (range_end > range_start),
    CONSTRAINT item_id_reservations_start_positive CHECK (range_start > 0),
    CONSTRAINT item_id_reservations_channel_nonnegative CHECK (channel_id >= 0),
    CONSTRAINT item_id_reservations_generation_positive CHECK (generation > 0)
);

CREATE INDEX item_id_reservations_channel_generation
    ON item_id_reservations (channel_id, generation);
