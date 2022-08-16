#!/bin/bash

set -euxo pipefail

RUST_BACKTRACE=1 cargo miri test --all
RUST_BACKTRACE=1 cargo miri test --examples