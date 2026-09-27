#!/usr/bin/env bash
#
# Delete a pull request's image tag from the GitHub Container Registry.
#
# Called by the cleanup-pr-image job in .github/workflows/docker.yml when a pull
# request is merged or closed, so a pull request image never outlives the branch
# that produced it.
#
# Environment:
#   GITHUB_REPOSITORY  owner/name of the repository
#   GITHUB_ACTOR       registry username, paired with GH_TOKEN
#   GH_TOKEN           token that can write to the package
#   PR_NUMBER          pull request number
#   HEAD_REF           branch the pull request came from
#   HEAD_REPO          owner/name the pull request came from
#
# Exits non-zero only when the tag is in the registry and cannot be deleted.

set -uo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

repo=$(printf '%s' "$GITHUB_REPOSITORY" | tr '[:upper:]' '[:lower:]')
image="ghcr.io/$repo"

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

# Pull requests from forks never got an image pushed.
if [ "${HEAD_REPO:-}" != "$GITHUB_REPOSITORY" ]; then
	report "PR #$PR_NUMBER is from ${HEAD_REPO:-unknown}, no image was published for it."
	exit 0
fi

tag=$("$script_dir/pr-image-tag.sh" "$PR_NUMBER" "$HEAD_REF")

# Only ever delete a pull request image, whatever the tag script hands back.
case "$tag" in
pr-*) ;;
*)
	report "::error::Refusing to delete $image:$tag, that is not a pull request image tag"
	exit 1
	;;
esac

token=$(curl -fsS -u "${GITHUB_ACTOR}:${GH_TOKEN}" \
	"https://ghcr.io/token?service=ghcr.io&scope=repository:${repo}:pull,push" |
	jq -r '.token')
if [ -z "$token" ] || [ "$token" = "null" ]; then
	report "::error::Could not get a registry token for $repo"
	exit 1
fi

manifest_types='application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json'

# The digest behind a tag. Empty when the tag is not in the registry, which is
# safe to read as absent: the token request above already reached the registry.
digest=$(curl -fsS -o /dev/null -D - \
	-H "Authorization: Bearer $token" \
	-H "Accept: $manifest_types" \
	"https://ghcr.io/v2/${repo}/manifests/${tag}" 2>/dev/null |
	tr -d '\r' |
	awk 'tolower($1) == "docker-content-digest:" { print $2 }')

if [ -z "$digest" ]; then
	report "$image:$tag was not in the registry"
	exit 0
fi

code=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE \
	-H "Authorization: Bearer $token" \
	"https://ghcr.io/v2/${repo}/manifests/${digest}")
case "$code" in
2*) report "Deleted $image:$tag ($digest)" ;;
404 | 405) report "$image:$tag was already gone" ;;
*)
	report "::error::Could not delete $image:$tag (HTTP $code)"
	exit 1
	;;
esac
