#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
cd "$repository_root"

if command -v mise >/dev/null 2>&1; then
  mise_bin=$(command -v mise)
elif [ -n "${HOME:-}" ] && [ -x "$HOME/.local/bin/mise" ]; then
  mise_bin="$HOME/.local/bin/mise"
else
  mise_bin=
fi

if [ -n "$mise_bin" ]; then
  exec "$mise_bin" exec -- npm --prefix editors/vscode run build
fi

if command -v npm >/dev/null 2>&1; then
  exec npm --prefix editors/vscode run build
fi

printf '%s\n' 'Could not find mise or npm. Install the repository toolchain with mise, or make npm available on PATH.' >&2
exit 127
