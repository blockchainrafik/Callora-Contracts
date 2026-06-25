#!/usr/bin/env bash
# Verify that every env.events().publish( site in the three contract crates
# has a matching topic entry in EVENT_SCHEMA.md.
#
# Usage:
#   ./scripts/check_event_schema_coverage.sh
#   SCHEMA_FILE=docs/MY_SCHEMA.md ./scripts/check_event_schema_coverage.sh
#
# Exit codes:
#   0  all topics are documented
#   1  one or more topics are missing from EVENT_SCHEMA.md

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCHEMA_FILE="${SCHEMA_FILE:-"${REPO_ROOT}/EVENT_SCHEMA.md"}"
CONTRACTS_DIR="${REPO_ROOT}/contracts"

if [[ ! -t 1 ]]; then
  RED=''
  GREEN=''
  YELLOW=''
  NC=''
fi

if [[ ! -f "${SCHEMA_FILE}" ]]; then
  echo -e "${RED}ERROR:${NC} schema file not found: ${SCHEMA_FILE}" >&2
  exit 1
fi

echo "Checking EVENT_SCHEMA.md coverage"
echo "  Schema : ${SCHEMA_FILE}"
echo "  Crates : ${CONTRACTS_DIR}/*/src/lib.rs"
echo ""

# Strip #[cfg(test)] blocks from a lib.rs file, then extract all Symbol::new
# topic strings that appear in publish calls. Test blocks are excluded because
# they may reference topic names that are not real contract events.
collect_topics() {
  local lib="$1"

  awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { inside_test = 1; depth = 0; next }
    inside_test {
      n = split($0, chars, "")
      for (i = 1; i <= n; i++) {
        if (chars[i] == "{") depth++
        if (chars[i] == "}") {
          depth--
          if (depth <= 0) { inside_test = 0; next }
        }
      }
      next
    }
    { print }
  ' "${lib}" \
  | grep -oP 'Symbol::new\(&env,\s*"\K[^"]+' \
  | sort -u
}

declare -A ALL_TOPICS

for lib in "${CONTRACTS_DIR}"/*/src/lib.rs; do
  [[ -f "${lib}" ]] || continue
  crate=$(basename "$(dirname "$(dirname "${lib}")")")

  while IFS= read -r topic; do
    [[ -z "${topic}" ]] && continue
    ALL_TOPICS["${topic}"]="${crate}"
  done < <(collect_topics "${lib}")
done

if [[ ${#ALL_TOPICS[@]} -eq 0 ]]; then
  echo -e "${YELLOW}WARN:${NC} no publish topics found under ${CONTRACTS_DIR}" >&2
  exit 0
fi

echo "Found ${#ALL_TOPICS[@]} unique topic(s) across all crates:"
for t in $(printf '%s\n' "${!ALL_TOPICS[@]}" | sort); do
  echo "  [${ALL_TOPICS[$t]}]  ${t}"
done
echo ""

# A topic is considered documented if the schema file contains any of:
#   ### `topic_name`   (section header)
#   `topic_name`       (inline backtick reference)
#   "topic_name"       (double-quoted, e.g. in JSON examples)

missing=()

for topic in $(printf '%s\n' "${!ALL_TOPICS[@]}" | sort); do
  if grep -qE "(###[[:space:]]+\`${topic}\`|\`${topic}\`|\"${topic}\")" "${SCHEMA_FILE}"; then
    echo -e "  ${GREEN}OK${NC}    ${topic}"
  else
    echo -e "  ${RED}MISSING${NC}  ${topic}  (crate: ${ALL_TOPICS[$topic]})"
    missing+=("${topic}")
  fi
done

echo ""

if [[ ${#missing[@]} -gt 0 ]]; then
  echo -e "${RED}FAIL:${NC} ${#missing[@]} topic(s) not documented in EVENT_SCHEMA.md:"
  for t in "${missing[@]}"; do
    echo "  - ${t}  (crate: ${ALL_TOPICS[$t]})"
  done
  echo ""
  echo "  Add a section to EVENT_SCHEMA.md for each missing topic, then re-run."
  exit 1
fi

echo -e "${GREEN}OK:${NC} all ${#ALL_TOPICS[@]} topic(s) are documented in EVENT_SCHEMA.md."