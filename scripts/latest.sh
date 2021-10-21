#!/bin/bash

VOLUME=$(wg genkey | base64 -d | xxd -c 80 -ps)
curl -v http://localhost:8002/snapshot/$VOLUME/latest
