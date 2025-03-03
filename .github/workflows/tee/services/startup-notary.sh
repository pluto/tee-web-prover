#!/bin/bash
set -ex
source /root/.cargo/env

cargo build --release
