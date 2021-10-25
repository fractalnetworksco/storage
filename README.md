e# Storage API

This Rust project hosts a storage API that we use as the backend for our
volume management. During normal operation, the docker driver periodically
snapshots the volumes and pushes the data to the storage API.

Every snapshot has a *generation number*, which is a number that is
incremented every time a transaction is completed on the filesystem.
This number increments monotonically. We can represent the state of a snapshot
by the generation number.

There are two types of snapshots:
- Full snapshots reflect the entire state of the volume. These have the
  advantage that they can be used to restore the entire volume from scratch.
  The downside is that they take up a lot of storage, roughly as much as there
  is data in the volume, plus metadata.
- Partial or incremental snapshots only store the change of state between

