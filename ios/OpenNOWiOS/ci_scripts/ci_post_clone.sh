#!/bin/sh
# Prepare the repository-vendored native libraries offline after checkout.
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
python3 "$script_dir/../BuildScripts/prepare-native-libraries.py"
