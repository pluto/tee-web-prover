#!/bin/bash
set -ex
. /root/.cargo/env

cargo build --release

setcap 'cap_net_bind_service=+ep' target/release/tee-web-prover
