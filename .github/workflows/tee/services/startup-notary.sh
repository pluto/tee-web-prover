#!/bin/bash
set -ex

. "$HOME/.cargo/env"

cargo build --release
