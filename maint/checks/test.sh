#!/bin/bash

set -euxo pipefail

RUST_BACKTRACE=1 cargo test --all
RUST_BACKTRACE=1 cargo test --examples