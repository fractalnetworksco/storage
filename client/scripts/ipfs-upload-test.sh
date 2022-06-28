#!/bin/bash

PRIVKEY=$(./target/debug/storage-tool privkey)
FILE=$1
STORAGE_TOOL=./target/debug/storage-tool

echo "Generated privkey $PRIVKEY"
shasum $FILE
CID=$($STORAGE_TOOL ipfs-upload --privkey $PRIVKEY $FILE)
echo "Uploaded to IPFS as ipfs://$CID"
$STORAGE_TOOL ipfs-fetch --privkey $PRIVKEY $CID | shasum
