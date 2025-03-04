#!/bin/bash
set -ex
. /root/.cargo/env
cargo build --release

echo "acme_domain = \"$(cat /etc/tee/domain | tr -d '[:space:]')\"" >> .github/workflows/tee/notary.config.toml
