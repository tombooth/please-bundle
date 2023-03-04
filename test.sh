#!/bin/bash

set -euo pipefail

if [ "$(./target/debug/please-bundle | node)" = "bibble wibble" ]
then
	echo "yay"
	exit 0
else
	echo "boo"
	exit 1
fi
