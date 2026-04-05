#!/bin/bash
set -ex

npm run docs:build
docker compose up -d --force-recreate
