#!/usr/bin/env bash
# Fails the build if a `#[cfg(test)] mod ...;` declaration has been commented
# out alongside a FIXME marker (see #804) — that pattern hides a test module
# from CI silently, so it must not slip back in unnoticed.
#
# Originally scoped to contracts/ip_registry, the crate #804 covered; that
# crate has since been removed (SwapIT dropped the IP-registry contract in
# favor of being a pure atomic-swap platform). atomic_swap carries its own
# pre-existing disabled-test debt tracked separately and stays out of scope
# here, so this check is currently a no-op until a new target is chosen.
set -euo pipefail

TARGET_DIR="contracts/ip_registry"
if [ ! -d "$TARGET_DIR" ]; then
  echo "No target directory ($TARGET_DIR removed); nothing to check."
  exit 0
fi
found=0
while IFS= read -r entry; do
  file="${entry%%:*}"
  line="${entry#*:}"
  line="${line%%:*}"
  next_line=$(sed -n "$((line + 1))p" "$file")
  if echo "$next_line" | grep -qE '^\s*//\s*(#\[cfg\(test\)\]|mod\s)'; then
    echo "$file:$line: FIXME marker precedes a commented-out test module"
    found=1
  fi
done < <(grep -rn --include='*.rs' -E '//\s*FIXME' "$TARGET_DIR")

if [ "$found" -eq 1 ]; then
  exit 1
fi

echo "No disabled test modules found."
