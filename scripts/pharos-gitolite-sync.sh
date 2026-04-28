#!/usr/bin/env bash
set -euo pipefail

ROOT=${ROOT:-/Volumes/code/ianzepp}
ADMIN_REPO=${ADMIN_REPO:-/tmp/pharos-gitolite-admin}
REMOTE_NAME=${REMOTE_NAME:-pharos}
REMOTE_PREFIX=${REMOTE_PREFIX:-git@pharos:}
OWNER=${OWNER:-ianzepp}
DRY_RUN=1

usage() {
  cat <<USAGE
usage: $0 [--apply] [--root PATH]

Creates/updates Gitolite repo grants for top-level Git repos under ROOT and
adds a local Git remote named "$REMOTE_NAME" pointing at git@pharos:<repo>.git.

Environment:
  ROOT=$ROOT
  ADMIN_REPO=$ADMIN_REPO
  REMOTE_NAME=$REMOTE_NAME
  OWNER=$OWNER
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --apply) DRY_RUN=0 ;;
    --dry-run) DRY_RUN=1 ;;
    --root)
      ROOT=${2:?missing path after --root}
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '+ %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

ensure_admin_repo() {
  if [ -d "$ADMIN_REPO/.git" ]; then
    run git -C "$ADMIN_REPO" pull --ff-only
  else
    run git clone git@pharos:gitolite-admin.git "$ADMIN_REPO"
  fi
}

list_repos() {
  declare -A seen_common_dirs=()
  find "$ROOT" -mindepth 1 -maxdepth 1 -type d -print | sort |
    while IFS= read -r dir; do
      if git -C "$dir" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        common_dir=$(cd "$dir" && git rev-parse --path-format=absolute --git-common-dir)
        if [ -n "${seen_common_dirs[$common_dir]:-}" ]; then
          echo "skip linked worktree: $(basename "$dir") shares git dir with ${seen_common_dirs[$common_dir]}" >&2
          continue
        fi
        seen_common_dirs[$common_dir]=$(basename "$dir")
        basename "$dir"
      fi
    done
}

repo_exists_in_conf() {
  local repo=$1
  grep -Eq "^[[:space:]]*repo[[:space:]]+$repo([[:space:]]|$)" "$ADMIN_REPO/conf/gitolite.conf"
}

append_repo_to_conf() {
  local repo=$1
  {
    printf '\n'
    printf 'repo %s\n' "$repo"
    printf '    RW+     =   %s\n' "$OWNER"
  } >> "$ADMIN_REPO/conf/gitolite.conf"
}

sync_remote() {
  local repo=$1
  local dir=$ROOT/$repo
  local url=${REMOTE_PREFIX}${repo}.git

  if git -C "$dir" remote get-url "$REMOTE_NAME" >/dev/null 2>&1; then
    current=$(git -C "$dir" remote get-url "$REMOTE_NAME")
    if [ "$current" = "$url" ]; then
      echo "remote ok: $repo -> $url"
    elif [[ "$current" == git@pharos:* ]]; then
      echo "remote update: $repo $current -> $url"
      run git -C "$dir" remote set-url "$REMOTE_NAME" "$url"
    else
      echo "remote skip: $repo has $REMOTE_NAME=$current" >&2
    fi
  else
    echo "remote add: $repo -> $url"
    run git -C "$dir" remote add "$REMOTE_NAME" "$url"
  fi
}

main() {
  if [ ! -d "$ROOT" ]; then
    echo "missing root: $ROOT" >&2
    exit 1
  fi

  mapfile -t repos < <(list_repos)
  echo "found ${#repos[@]} repos under $ROOT"

  ensure_admin_repo
  if [ "$DRY_RUN" -eq 1 ] && [ ! -d "$ADMIN_REPO/.git" ]; then
    echo "dry run cannot inspect missing admin repo; run once after cloning or use --apply" >&2
    exit 1
  fi

  changed_conf=0
  for repo in "${repos[@]}"; do
    if repo_exists_in_conf "$repo"; then
      echo "gitolite ok: $repo"
    else
      echo "gitolite add: $repo"
      if [ "$DRY_RUN" -eq 0 ]; then
        append_repo_to_conf "$repo"
      else
        printf '+ append repo %s to %s\n' "$repo" "$ADMIN_REPO/conf/gitolite.conf"
      fi
      changed_conf=1
    fi
    sync_remote "$repo"
  done

  if [ "$changed_conf" -eq 1 ]; then
    if [ "$DRY_RUN" -eq 0 ]; then
      git -C "$ADMIN_REPO" add conf/gitolite.conf
      git -C "$ADMIN_REPO" commit -m "Add Pharos Gitolite repos"
      git -C "$ADMIN_REPO" push
    else
      echo '+ commit and push gitolite-admin changes'
    fi
  else
    echo "gitolite config already up to date"
  fi
}

main
