-- Add migration script here
DROP TABLE storage_volume;
DROP TABLE storage_snapshot;

CREATE TABLE storage_volume(
    volume_id INTEGER PRIMARY KEY NOT NULL,
    -- public key that identifies this volume
    volume_pubkey BLOB NOT NULL UNIQUE,
    -- account that created this volume
    account_id UUID NOT NULL
);

CREATE TABLE storage_snapshot(
    snapshot_id INTEGER PRIMARY KEY NOT NULL,
    -- volume this snapshot belongs to
    volume_id INTEGER NOT NULL REFERENCES storage_volume(volume_id) ON DELETE CASCADE,
    -- manifest of this snapshot
    snapshot_manifest BLOB NOT NULL,
    -- signature of manifest
    snapshot_signature BLOB NOT NULL,
    -- manifest hash (used as unique identifier)
    snapshot_hash BLOB UNIQUE NOT NULL,
    -- pointer to parent snapshot
    snapshot_parent INTEGER REFERENCES storage_snapshot(snapshot_id) ON DELETE CASCADE,
    -- this snapshot is replicated
    snapshot_replicated INTEGER NOT NULL DEFAULT 0
);
