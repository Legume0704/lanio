#!/usr/bin/env bash
#
# Docker tag for a pull request image: the pull request number and the branch
# name, for example pr-26-fix-year.
#
# The tag job and the cleanup job in .github/workflows/docker.yml both call this,
# so the tag a pull request is published under and the tag deleted when it
# closes can never drift apart. Change the rule here, nowhere else.
#
# Usage: scripts/pr-image-tag.sh <pr-number> <branch-name>
#
# Prints the tag on stdout.

set -euo pipefail

if [ "$#" -ne 2 ]; then
	echo "usage: $(basename "$0") <pr-number> <branch-name>" >&2
	exit 2
fi

pr_number=$1
branch=$2

if ! printf '%s' "$pr_number" | grep -Eq '^[0-9]+$'; then
	echo "not a pull request number: $pr_number" >&2
	exit 2
fi

# Docker tags match [a-z0-9_][a-z0-9_.-]{0,127}. The pr- prefix keeps a pull
# request image clear of the release, latest, test and build cache tags.
slug=$(
	printf '%s' "$branch" |
		tr '[:upper:]' '[:lower:]' |
		tr -c 'a-z0-9_.-' '-' |
		sed -e 's/-\{2,\}/-/g' -e 's/^-*//' -e 's/-*$//'
)

# A branch of "---" or similar leaves nothing behind.
[ -n "$slug" ] || slug=branch

# Branches only differing past the cut still need distinct tags.
if [ "${#slug}" -gt 100 ]; then
	hash=$(printf '%s' "$branch" | git hash-object --stdin | cut -c1-8)
	slug="$(printf '%s' "$slug" | cut -c1-91)-$hash"
fi

printf 'pr-%s-%s\n' "$pr_number" "$slug"
