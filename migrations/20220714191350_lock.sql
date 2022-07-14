-- Adds the volume writer field. When null, it is set to the UUID of the
-- first machine that is pushing a snapshot. When not null, snapshots from
-- any other machines are rejected.
ALTER TABLE storage_volume
    ADD COLUMN volume_writer UUID;
-- Determines if the volume is locked. Defaults to false, when true, snapshots
-- are rejected.
ALTER TABLE storage_volume
    ADD COLUMN volume_locked INTEGER NOT NULL DEFAULT 0;
