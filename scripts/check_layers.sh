#!/usr/bin/env bash
# check_layers.sh - CI gate for per-crate partial-order import rules.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

FAIL=0

crate_names=(
  agogo-chan
  agogo-core
  agogo-host-cpal
  agogo-host-link
  agogo-host-midi
  agogo-cli
)

crate_roots=(
  crates/chan/src
  crates/core/src
  crates/host-cpal/src
  crates/host-link/src
  crates/host-midi/src
  crates/cli/src
)

crate_layers=(
  "channel conn control sink test time"
  "bridge driver event runtime snapshot transport"
  cpal
  "link quantum session source transport"
  midir
  "command parse"
)

crate_import_roots=(
  "crate agogo_chan agogo::chan"
  "crate agogo_core agogo::core"
  "crate agogo_host_cpal agogo::host::cpal"
  "crate agogo_host_link agogo::host::link"
  "crate agogo_host_midi agogo::host::midi"
  "crate agogo_cli"
)

parse_deps() {
    local root="$1"
    local layer="$2"
    local file="${root}/${layer}.rs"

    if [[ ! -f "$file" ]]; then
        printf 'check_layers.sh: missing module-root file %s\n' "$file" >&2
        exit 2
    fi

    local declared
    declared=$(grep -m1 -E '^//! layer:' "$file" || true)
    if [[ -z "$declared" ]]; then
        printf 'check_layers.sh: %s missing `//! layer:` sentinel\n' "$file" >&2
        exit 2
    fi
    declared="${declared#*layer:}"
    declared="$(echo "$declared" | xargs)"
    if [[ "$declared" != "$layer" ]]; then
        printf 'check_layers.sh: %s declares layer `%s`, expected `%s`\n' \
            "$file" "$declared" "$layer" >&2
        exit 2
    fi

    local line
    line=$(grep -m1 -E '^//! depends-on:' "$file" || true)
    if [[ -z "$line" ]]; then
        printf 'check_layers.sh: %s missing `//! depends-on:` sentinel\n' "$file" >&2
        exit 2
    fi
    line="${line#*depends-on:}"
    line="${line//,/ }"
    line="$(echo "$line" | xargs)"
    printf '%s' "$line"
}

files_for_layer() {
    local root="$1"
    local layer="$2"
    local file="${root}/${layer}.rs"
    [[ -f "$file" ]] && printf '%s\n' "$file"
    [[ -d "${root}/${layer}" ]] && find "${root}/${layer}" -type f -name '*.rs' -print
}

emit_import_hits() {
    local file="$1"
    local root_re="$2"
    local line line_num=0 start_line=0 collecting=0 block=""
    local use_re="^(pub([[:space:]]*\\([^)]*\\))?[[:space:]]+)?use[[:space:]]+(${root_re})::"
    local grouped_re="use[[:space:]]+(${root_re})::\\{"

    while IFS= read -r line || [[ -n "$line" ]]; do
        ((line_num += 1))
        if (( collecting )); then
            block+=" $line"
            if [[ "$line" == *";"* ]]; then
                printf '%s:%s\n' "$start_line" "$block"
                collecting=0
                block=""
            fi
            continue
        fi

        if [[ "$line" =~ $use_re ]]; then
            if [[ "$line" =~ $grouped_re ]] &&
                [[ ! "$line" =~ \} ]]; then
                collecting=1
                start_line="$line_num"
                block="$line"
                continue
            fi
            printf '%s:%s\n' "$line_num" "$line"
        fi
    done <"$file"

    if (( collecting )); then
        printf '%s:%s\n' "$start_line" "$block"
    fi
}

layer_in_list() {
    local candidate="$1"
    shift
    local layer
    for layer in "$@"; do
        [[ "$candidate" == "$layer" ]] && return 0
    done
    return 1
}

emit_import_tops() {
    local line_body="$1"
    local root_re="$2"
    local rest item

    line_body="${line_body//$'\n'/ }"
    if [[ "$line_body" =~ use[[:space:]]+(${root_re})::\{(.*)\} ]]; then
        rest="${BASH_REMATCH[2]}"
        while [[ "$rest" =~ \{[^{}]*\} ]]; do
            rest="${rest//${BASH_REMATCH[0]}/}"
        done
        rest="${rest//,/ }"
        for item in $rest; do
            item="${item%%::*}"
            item="${item%%;*}"
            item="${item%% as *}"
            [[ "$item" =~ ^[a-z][a-z0-9_]*$ ]] && printf '%s\n' "$item"
        done
        return
    fi

    if [[ "$line_body" =~ use[[:space:]]+(${root_re})::([a-z][a-z0-9_]*) ]]; then
        printf '%s\n' "${BASH_REMATCH[2]}"
    fi
}

for i in "${!crate_names[@]}"; do
    crate="${crate_names[$i]}"
    root="${crate_roots[$i]}"
    root_re="${crate_import_roots[$i]// /|}"
    # shellcheck disable=SC2206
    layers=(${crate_layers[$i]})

    for layer in "${layers[@]}"; do
        deps=$(parse_deps "$root" "$layer")
        authorised=("$layer")
        if [[ -n "$deps" ]]; then
            # shellcheck disable=SC2206
            authorised+=($deps)
        fi

        while IFS= read -r file; do
            while IFS= read -r hit; do
                line_num="${hit%%:*}"
                line_body="${hit#*:}"
                while IFS= read -r top; do
                    [[ -z "$top" ]] && continue
                    layer_in_list "$top" "${layers[@]}" || continue
                    layer_in_list "$top" "${authorised[@]}" && continue

                    printf '%s:%s - %s:%s imports %s which is not in %s'\''s depends-on list\n' \
                        "$file" "$line_num" "$crate" "$layer" "$top" "$layer" >&2
                    printf '    %s\n' "$line_body" >&2
                    FAIL=1
                done < <(emit_import_tops "$line_body" "$root_re")
            done < <(emit_import_hits "$file" "$root_re")
        done < <(files_for_layer "$root" "$layer")
    done
done

if (( FAIL )); then
    cat >&2 <<'HINT'

check_layers.sh: FAIL.

The partial-order layering rule keeps inter-module imports
unidirectional. To resolve:

  - If the import is genuinely cross-cutting, lift the shared type to a lower layer.
  - If the import is stale from a refactor, delete it.
  - If a layer edge is truly needed, update the module-root
    `//! depends-on:` sentinel and AGENTS.md together.

HINT
    exit 1
fi

printf 'check_layers.sh: OK - workspace module imports respect the partial orders.\n'
