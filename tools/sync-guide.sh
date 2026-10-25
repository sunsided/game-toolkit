#!/usr/bin/env bash
# Stamp the canonical guide, templates/jam-agent-guide.md, into its generated copies:
#
#   - templates/jam/CLAUDE.md and templates/jam-3d/CLAUDE.md: the whole guide, for coding
#     agents in generated jam projects;
#   - crates/prelude/src/guide.md: the guide minus the <!-- agent-only --> blocks, with Rust
#     code blocks marked `ignore`, published as the `game_toolkit::guide` module on docs.rs.
#
# Marker lines (<!-- agent-only --> / <!-- /agent-only -->) and the leading
# `<!-- source note ... -->` comment never reach either copy.
#
# Usage: tools/sync-guide.sh          rewrite the copies
#        tools/sync-guide.sh --check  fail if a copy is out of date (CI, `task check:templates`)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/templates/jam-agent-guide.md"

agent_copy() {
  sed -e '/^<!-- source note$/,/^-->$/d' \
      -e '/^<!-- \/\{0,1\}agent-only -->$/d' "$src"
}

crate_copy() {
  sed -e '/^<!-- source note$/,/^-->$/d' \
      -e '/^<!-- agent-only -->$/,/^<!-- \/agent-only -->$/d' \
      -e 's/^```rust$/```rust,ignore/' "$src"
}

check=0
[ "${1:-}" = "--check" ] && check=1
status=0

emit() {
  local dest="$1" generator="$2"
  if [ "$check" = 1 ]; then
    if ! cmp -s <("$generator") "$root/$dest"; then
      echo "$dest is out of sync with templates/jam-agent-guide.md; run: task sync:templates" >&2
      status=1
    fi
  else
    "$generator" > "$root/$dest"
  fi
}

emit templates/jam/CLAUDE.md agent_copy
emit templates/jam-3d/CLAUDE.md agent_copy
emit crates/prelude/src/guide.md crate_copy
exit "$status"
