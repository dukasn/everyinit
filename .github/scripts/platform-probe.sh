#!/usr/bin/env bash

set -uo pipefail

report_dir="platform-probe-report"
mkdir -p "$report_dir"
export RUST_BACKTRACE=short

results_file="$report_dir/results.tsv"
: > "$results_file"

{
  printf 'commit: %s\n' "$(git rev-parse HEAD 2>&1)"
  printf 'runner_os: %s\n' "${RUNNER_OS:-unknown}"
  printf 'runner_arch: %s\n' "${RUNNER_ARCH:-unknown}"
  printf 'runner_name: %s\n' "${ImageOS:-unknown} ${ImageVersion:-}"
  printf 'rustc:\n'
  rustc -Vv 2>&1 || true
  printf '\ncargo: '
  cargo -V 2>&1 || true
  printf '\nactive_toolchain: '
  rustup show active-toolchain 2>&1 || true
  printf '\nhost: '
  uname -a 2>&1 || true
} | tee "$report_dir/environment.txt"

run_probe() {
  local id="$1"
  local label="$2"
  shift 2
  local log="$report_dir/$id.log"
  local status

  printf '\n## %s\n' "$label"
  printf 'command: '
  printf '%q ' "$@"
  printf '\n'
  "$@" 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  printf '%s\t%s\t%s\n' "$id" "$label" "$status" >> "$results_file"
  printf 'exit status: %s\n' "$status"
}

run_probe check-all-targets \
  'Compile workspace and all targets' \
  cargo check --workspace --all-targets --locked

run_probe unit-tests \
  'Library and binary unit tests (continue after failing test binaries)' \
  cargo test --workspace --lib --bins --locked --no-fail-fast -- --show-output

run_probe integration-tests \
  'Integration tests (continue after failing test binaries)' \
  cargo test --workspace --tests --locked --no-fail-fast -- --show-output

run_probe documentation-tests \
  'Documentation tests (continue after failing test binaries)' \
  cargo test --workspace --doc --locked --no-fail-fast -- --show-output

summary_file="$report_dir/summary.md"
failure_index="$report_dir/failure-index.txt"
{
  for log in "$report_dir"/{check-all-targets,unit-tests,integration-tests,documentation-tests}.log; do
    [[ -f "$log" ]] || continue
    probe_id="${log##*/}"
    probe_id="${probe_id%.log}"
    status="$(awk -F '\t' -v id="$probe_id" '$1 == id { print $3; exit }' "$results_file")"
    [[ "$status" != 0 ]] || continue
    printf '\n### %s\n' "${log##*/}"
    if ! grep -nE '^error(\[[^]]+\])?:|^---- .* stdout ----$|^test .* \.\.\. FAILED$|^test result: FAILED|^error: test failed, to rerun pass' "$log"; then
      printf 'No matching compiler error or failed-test summary lines. See the full log for details.\n'
    fi
  done
} | tee "$failure_index"

{
  printf '# Platform probe report\n\n'
  printf -- '- Commit: `%s`\n' "$(git rev-parse HEAD)"
  printf -- '- Runner: `%s` / `%s`\n' "${RUNNER_OS:-unknown}" "${RUNNER_ARCH:-unknown}"
  printf -- '- Run: `%s`\n\n' "${GITHUB_RUN_ID:-local}"
  printf '| Probe | Result | Exit code |\n| --- | --- | ---: |\n'
  while IFS="$(printf '\t')" read -r id label status; do
    if [[ "$status" == 0 ]]; then
      result='PASS'
    else
      result='FAIL'
    fi
    printf '| %s | %s | %s |\n' "$label" "$result" "$status"
  done < "$results_file"
  printf '\n`failure-index.txt` lists compiler errors and failed-test summaries from failed probes. Full command output is in the matching `.log` files. `environment.txt` records the runner and Rust toolchain.\n'
} | tee "$summary_file"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  cat "$summary_file" >> "$GITHUB_STEP_SUMMARY"
fi

if awk -F '\t' '$3 != 0 { failed = 1 } END { exit !failed }' "$results_file"; then
  exit 1
fi
