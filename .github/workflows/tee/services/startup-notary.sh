#!/bin/bash
set -ex
. /root/.cargo/env

cargo build --release
