#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: scripts/check-commit-range.sh BASE_SHA HEAD_SHA"
    exit 2
fi

base="${1}"
head="${2}"

if [[ "${base}" =~ ^0+$ ]]; then
    range="${head}"
else
    range="${base}..${head}"
fi

while IFS=$'\t' read -r sha subject; do
    [[ -z "${sha}" ]] && continue
    echo "Checking ${sha}: ${subject}"
    bash scripts/check-commit-message.sh --message "${subject}"
done < <(git log --format='%H%x09%s' "${range}")
