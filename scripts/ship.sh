#!/usr/bin/env bash
# Ship a version: bump, commit and push, build locally, publish the GitHub
# release (with the auto-update files). Apple silicon only for now.
#
#   scripts/ship.sh 1.0.4                    # notes drafted from commits since the last tag
#   scripts/ship.sh 1.0.4 --edit             # same, opened in $EDITOR first
#   scripts/ship.sh 1.0.4 --notes notes.md   # your own notes (Markdown)
#   scripts/ship.sh 1.0.4 --dry-run          # checks and notes only; changes nothing
#
# Everything is checked before anything changes. If the build or upload fails
# after the version is pushed, fix it and run the same command again: the
# bump is skipped and it carries on from the build.
#
# Needs: the updater key (~/.tauri/mystique.key, password in the keychain item
# "mystique-updater-key") and a GitHub login that can push to the repo
# (GH_TOKEN, else `gh auth token -u yash1200`). See docs/RELEASING.md.
set -euo pipefail

REPO="yash1200/mystique"
GH_USER="yash1200"
KEY_FILE="${HOME}/.tauri/mystique.key"
KEYCHAIN_ITEM="mystique-updater-key"

cd "$(dirname "$0")/.."

die() {
	echo "ship: $*" >&2
	exit 1
}
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

usage() {
	sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
	exit "${1:-0}"
}

# --- arguments ---------------------------------------------------------------
version=""
notes_file=""
edit=no
dry_run=no
while [[ $# -gt 0 ]]; do
	case "$1" in
	-h | --help) usage 0 ;;
	--notes)
		[[ $# -ge 2 ]] || die "--notes needs a file"
		notes_file="$2"
		shift 2
		;;
	--edit)
		edit=yes
		shift
		;;
	--dry-run)
		dry_run=yes
		shift
		;;
	-*) die "unknown option $1 (see --help)" ;;
	*)
		[[ -z "$version" ]] || die "one version only"
		version="${1#v}"
		shift
		;;
	esac
done
[[ -n "$version" ]] || usage 1
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "\"$version\" is not a version (x.y.z)"
[[ -z "$notes_file" || -f "$notes_file" ]] || die "no notes file at $notes_file"
[[ "$(uname -s)" == Darwin ]] || die "releases are built on a Mac"
tag="v${version}"

current=$(node -p "require('./src-tauri/tauri.conf.json').version")

# --- checks (nothing changes before these pass) --------------------------------
step "Checking"

[[ "$(git rev-parse --abbrev-ref HEAD)" == main ]] || die "not on main"
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "the working tree has changes; commit or stash them"
git fetch --quiet origin main --tags
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "HEAD is not origin/main; push or pull first"

# Resuming: the version is already bumped and pushed, but not released.
resume=no
if [[ "$current" == "$version" ]]; then
	resume=yes
elif [[ "$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -n 1)" != "$version" ]]; then
	die "$version is not newer than the current $current"
fi

git rev-parse -q --verify "refs/tags/${tag}" >/dev/null && die "tag ${tag} already exists"

if [[ -z "${GH_TOKEN:-}" ]]; then
	GH_TOKEN=$(gh auth token -u "$GH_USER" 2>/dev/null) || die "gh has no login for ${GH_USER}; run: gh auth login"
	export GH_TOKEN
fi
[[ "$(gh api "repos/${REPO}" --jq .permissions.push 2>/dev/null)" == true ]] || die "this GitHub login cannot push to ${REPO}"
gh release view "$tag" --repo "$REPO" >/dev/null 2>&1 && die "release ${tag} already exists on GitHub"

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
	[[ -f "$KEY_FILE" ]] || die "no updater key at ${KEY_FILE}"
fi
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+set}" ]]; then
	security find-generic-password -s "$KEYCHAIN_ITEM" >/dev/null 2>&1 ||
		die "no keychain item ${KEYCHAIN_ITEM} with the updater key password"
fi

echo "current ${current} → ${version}$([[ $resume == yes ]] && echo ' (already bumped; resuming)')"
echo "GitHub: ${REPO}, as $(gh api user --jq .login)"

# --- release notes -----------------------------------------------------------
notes_out="target/release-notes-${tag}.md"
mkdir -p target
if [[ -n "$notes_file" ]]; then
	cp "$notes_file" "$notes_out"
else
	last_tag=$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)
	range="${last_tag:+${last_tag}..}HEAD"
	changes=$(git log --no-merges --format='- %s' "$range" |
		grep -Ev '^- chore\(release\)' |
		sed -E 's/^- (feat|fix|perf|refactor|docs|chore|test)(\([^)]*\))?: /- /' || true)
	{
		echo "## Changes"
		echo
		echo "${changes:-- Maintenance release.}"
		echo
		echo "## Install"
		echo
		echo "Apple silicon only. Mystique 1.0.3 and later update themselves; on an older version, download the .dmg below once."
		echo
		echo "This build is not notarized: on first launch, right-click Mystique in Applications and choose **Open**."
	} >"$notes_out"
	if [[ "$edit" == yes ]]; then
		"${EDITOR:-vi}" "$notes_out"
	fi
fi
step "Release notes (${notes_out})"
cat "$notes_out"

if [[ "$dry_run" == yes ]]; then
	echo
	echo "Dry run: checks passed. Nothing was bumped, built or published."
	exit 0
fi

# --- bump, commit, push ------------------------------------------------------
if [[ "$resume" == no ]]; then
	step "Bumping to ${version}"
	pnpm -s bump "$version"
	git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json Cargo.lock
	git commit -q -m "chore(release): version ${version}"
	git push -q origin main
	echo "pushed $(git rev-parse --short HEAD) chore(release): version ${version}"
fi

# --- build and publish -------------------------------------------------------
step "Building and publishing ${tag}"
node scripts/release.mjs --notes "$notes_out"

step "Done"
echo "https://github.com/${REPO}/releases/tag/${tag}"
