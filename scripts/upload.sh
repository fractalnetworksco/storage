#!/bin/bash

VOLUME=$(wg genkey | base64 -d | xxd -c 80 -ps)
curl -v -X POST -H "Content-Type: text/plain" -d @$1 http://localhost:8002/snapshot/$VOLUME/upload
