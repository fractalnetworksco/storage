# Allow multiple uploads of the same snapshot to avoid unique contraint errors

Transiant errors of volume snapshots lead to unique constraint violations, this MR
allows storage clients to upload the same snapshot volume as long as the manifest is unmodified.

## Validation

We have added a test to make sure that the same snapshot can be uploaded twice without error.

`cargo test`
