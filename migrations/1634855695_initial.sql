CREATE TABLE storage_volume(
    volume_id INTEGER PRIMARY KEY NOT NULL,
    volume_pubkey BLOB NOT NULL UNIQUE,
    volume_user BLOB
);

CREATE TABLE storage_snapshot(
    volume_id INTEGER NOT NULL,
    snapshot_generation INTEGER NOT NULL,
    snapshot_parent INTEGER,
    snapshot_size INTEGER NOT NULL,
    snapshot_time INTEGER NOT NULL,
    PRIMARY KEY (volume_id, snapshot_generation, snapshot_parent)
);
