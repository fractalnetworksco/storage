# Storage API

This Rust project hosts the storage API, which implements a fully end-to-end
encrypted and signed blob storage with metadata. Currently, the metadata is
stored inside of an SQLite database, and the data on disk, however in the
future this will support different backends (S3-compatible, filecoin, etc).

Building Locally

Insall deps
```
sudo apt install libssl-dev pkg-config
```
Build
```
cargo build --release
```



Builds:
- [storage-master-amd64][] ([signature][storage-master-amd64.sig])
- [storage-master-arm64][] ([signature][storage-master-arm64.sig])
- [storage-master-arm32][] ([signature][storage-master-arm32.sig])

Containers:
- [`registry.gitlab.com/fractalnetworks/storage`][registry]

Resources:
- [Source Documentation][rustdoc]
- [API Documentation][openapi] (TODO)

## Background

We use this as the backend for our
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
- Partial or incremental snapshots only store the change of state between two
  versions

[storage-master-amd64]: https://fractalnetworks.gitlab.io/storage/storage-master-amd64
[storage-master-arm64]: https://fractalnetworks.gitlab.io/storage/storage-master-arm64
[storage-master-arm32]: https://fractalnetworks.gitlab.io/storage/storage-master-arm32

[storage-master-amd64.sig]: https://fractalnetworks.gitlab.io/storage/storage-master-amd64.sig
[storage-master-arm64.sig]: https://fractalnetworks.gitlab.io/storage/storage-master-arm64.sig
[storage-master-arm32.sig]: https://fractalnetworks.gitlab.io/storage/storage-master-arm32.sig

[rustdoc]: https://fractalnetworks.gitlab.io/storage/doc/storage
[openapi]: https://fractalnetworks.gitlab.io/storage/api
[registry]: https://gitlab.com/fractalnetworks/storage/container_registry

