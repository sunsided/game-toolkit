#!/usr/bin/env bash
# Generate each cargo-generate jam template and run the checks a fresh project would face,
# against this checkout of the toolkit (not the git `main` the templates normally pull).
#
# Per template:
#   - generate with a one-letter and a long project name, and `cargo fmt --check` both
#     (the name is spliced into source, so rustfmt's line breaking can depend on its length);
#   - clippy the long-name project with -D warnings, natively and (if the target is
#     installed) for wasm32-unknown-unknown.
#
# Used by CI and `task test:templates`. Needs cargo-generate.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Share the workspace target dir so the toolkit's dependencies are not rebuilt.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"

if rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  wasm=1
else
  wasm=0
  echo "note: wasm32-unknown-unknown not installed, skipping the wasm clippy pass" >&2
fi

for template in jam jam-3d; do
  for name in g my-rather-long-game-jam-project-name; do
    dest="$work/$template"
    mkdir -p "$dest"
    echo "::group::$template ($name)"
    cargo generate --path "$root/templates/$template" --name "$name" \
      --destination "$dest" --vcs none --silent
    project="$dest/$name"

    if [ ! -f "$project/Cargo.toml" ]; then
      echo "error: $template generated no Cargo.toml" >&2
      exit 1
    fi

    # Build against this checkout instead of the git dependency on `main`.
    cat >>"$project/Cargo.toml" <<EOF

[patch."https://github.com/sunsided/game-toolkit"]
game-toolkit = { path = "$root/crates/prelude" }
EOF

    (
      cd "$project"
      cargo fmt --check
      if [ "$name" != g ]; then
        cargo clippy --all-targets -- -D warnings
        if [ "$wasm" = 1 ]; then
          cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings
        fi
      fi
    )
    echo "::endgroup::"
  done
done

echo "templates ok"
