#!/bin/sh
# Xcode Cloud runs this beside the project after checking out the pushed commit.
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
python3 "$script_dir/../BuildScripts/prepare-native-libraries.py"
