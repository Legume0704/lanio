#!/usr/bin/env bash
#
# Delete a pull request's image tag from the GitHub Container Registry.
#
# Called by the cleanup-pr-image job in .github/workflows/docker.yml when a pull
# request is merged or closed, so a pull request image never outlives the branch
# that produced it.
#
# GitHub Packages has no delete on the registry API (DELETE /v2/<name>/manifests/
# <digest> answers 405), so this goes through the REST API: find the package
# version carrying the tag, then delete that version.
#
# Environment:
#   GITHUB_REPOSITORY  owner/name of the repository
#   GH_TOKEN           token that can read and delete packages
#   PR_NUMBER          pull request number
#   HEAD_REF           branch the pull request came from
#   HEAD_REPO          owner/name the pull request came from
#   PR_TAG             tag to remove, instead of deriving one from the pull request
#
# Exits non-zero when the tag is in the registry and cannot be deleted.

set -uo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

owner=${GITHUB_REPOSITORY%%/*}
repo_name=${GITHUB_REPOSITORY#*/}
image="ghcr.io/$owner/$repo_name"

# Say what happened, and put it in the job summary when running in Actions.
summary_started=0
report() {
	echo "$1"
	[ -n "${GITHUB_STEP_SUMMARY:-}" ] || return 0
	if [ "$summary_started" = 0 ]; then
		{
			echo "## Pull request image removed"
			echo
		} >> "$GITHUB_STEP_SUMMARY"
		summary_started=1
	fi
	echo "$1" >> "$GITHUB_STEP_SUMMARY"
}

# Call the GitHub API with the given method (GET by default), leaving the
# response in api_body and the status in api_status. Never fails, so a problem
# can be reported with GitHub's own explanation of it rather than a bare curl
# error.
api_body=""
api_status=0
api() {
	local method=${2:-GET} response
	response=$(curl -sS -X "$method" -w '\n%{http_code}' \
		-H "Authorization: Bearer $GH_TOKEN" \
		-H "Accept: application/vnd.github+json" \
		-H "X-GitHub-Api-Version: 2022-11-28" \
		"https://api.github.com$1")
	api_status=${response##*$'\n'}
	api_body=${response%$'\n'*}
	# A connection failure leaves no status behind, so do not test it as a number.
	case "$api_status" in
	'' | *[!0-9]*) return 1 ;;
	esac
	[ "$api_status" -lt 400 ]
}

api_error() {
	local message
	message=$(printf '%s' "$api_body" | jq -r '.message // empty' 2>/dev/null)
	[ -n "$api_status" ] || {
		echo "no response from the GitHub API"
		return
	}
	[ -z "$message" ] || {
		printf 'HTTP %s: %s' "$api_status" "$message"
		return
	}
	printf 'HTTP %s' "$api_status"
}

# Pull requests from forks never got an image pushed. A run started by hand is
# always about this repository, so it has no HEAD_REPO to compare against.
if [ -z "${PR_TAG:-}" ] && [ "${HEAD_REPO:-}" != "$GITHUB_REPOSITORY" ]; then
	report "PR #$PR_NUMBER is from ${HEAD_REPO:-unknown}, no image was published for it."
	exit 0
fi

# A run started by hand names the tag itself, since the pull request that owned
# the tag is long gone by then.
if [ -n "${PR_TAG:-}" ]; then
	tag=$PR_TAG
else
	tag=$("$script_dir/pr-image-tag.sh" "$PR_NUMBER" "$HEAD_REF")
fi

# Only ever delete a pull request image, whatever the tag script hands back.
case "$tag" in
pr-*) ;;
*)
	report "::error::Refusing to delete $image:$tag, that is not a pull request image tag"
	exit 1
	;;
esac

# A package is listed under its owner, which is a user or an organization.
if ! api "/repos/$owner/$repo_name"; then
	report "::error::Could not read $GITHUB_REPOSITORY ($(api_error))"
	exit 1
fi
if [ "$(printf '%s' "$api_body" | jq -r '.owner.type' | tr '[:upper:]' '[:lower:]')" = "organization" ]; then
	versions="/orgs/$owner/packages/container/$repo_name/versions"
else
	versions="/users/$owner/packages/container/$repo_name/versions"
fi

# The package version holding the tag, if it is still there. Newest first, so
# this normally lands on the first page.
version_id=""
page=1
while [ "$page" -le 20 ]; do
	if ! api "$versions?per_page=100&page=$page"; then
		report "::error::Could not list versions of package $repo_name ($(api_error))"
		exit 1
	fi
	version_id=$(printf '%s' "$api_body" |
		jq -r --arg tag "$tag" \
			'.[] | select((.metadata.container.tags // []) | index($tag)) | .id' |
		head -1)
	[ -n "$version_id" ] && break
	[ "$(printf '%s' "$api_body" | jq 'length')" -lt 100 ] && break
	page=$((page + 1))
done

if [ -z "$version_id" ]; then
	report "$image:$tag is not in the registry"
	exit 0
fi

if api "$versions/$version_id" DELETE; then
	report "Deleted $image:$tag (package version $version_id)"
else
	report "::error::Could not delete $image:$tag ($(api_error))"
	exit 1
fi
