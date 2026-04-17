#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  scripts/check-commit-message.sh COMMIT_MSG_FILE
  scripts/check-commit-message.sh --message "feat: add parser support"
EOF
}

subject=""

if [[ $# -eq 2 && "${1}" == "--message" ]]; then
    subject="${2}"
elif [[ $# -eq 1 ]]; then
    subject="$(head -n 1 "${1}" | tr -d '\r')"
else
    usage
    exit 2
fi

case "${subject}" in
    Merge\ *|Revert\ *|fixup!\ *|squash!\ *)
        exit 0
        ;;
esac

pattern='^(build|chore|ci|docs|feat|fix|perf|refactor|revert|style|test)(\([a-z0-9._/-]+\))?(!)?: .+$'

if [[ ! "${subject}" =~ ${pattern} ]]; then
    cat <<EOF
Invalid commit subject:
  ${subject}

Expected Conventional Commits:
  <type>[optional-scope][!]: <description>

Examples:
  fix: handle empty bookmark content
  feat(cli): add --stdout alias
  feat!: rename markdown heading format
EOF
    exit 1
fi

if [[ ${#subject} -gt 72 ]]; then
    echo "Commit subject is too long (${#subject} > 72): ${subject}"
    exit 1
fi
