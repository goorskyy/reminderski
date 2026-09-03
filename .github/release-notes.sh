#!/usr/bin/env bash
# The release notes for a tag: every commit since the previous tag, its subject as a heading and
# whatever explanation the commit already carries underneath.
#
# Nothing has to be remembered at commit time. A commit written with a body gets the space it
# deserves; one written without a body gets a heading and nothing else, which is all a typo fix
# needs.
#
# To read what a tag would say before pushing it:
#
#   bash .github/release-notes.sh v0.0.2
#
set -euo pipefail

tag=${1:?usage: release-notes.sh <tag>}

# The nearest tag before this one. There being none means this is the first release, and then
# everything up to the tag belongs in the notes.
previous=$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)
range=$tag
if [ -n "$previous" ]; then
    range="$previous..$tag"
fi

for commit in $(git rev-list --no-merges "$range"); do
    # Closing a milestone or rewording the rules changes nothing anybody downloaded. A commit
    # earns a place by touching something outside the documentation, which means anything new
    # counts from the day it is added without this having to be told about it.
    changed=$(git show --format='' --name-only "$commit" |
        grep -vE '^$|^docs/|^[^/]*\.md$' || true)
    if [ -z "$changed" ]; then
        continue
    fi

    printf '### %s\n\n' "$(git log -1 --format=%s "$commit")"

    # The first paragraph only. A commit here opens with what changed and then spends four more
    # paragraphs on why, which is written for whoever next reads the code rather than for
    # somebody deciding whether to download this. The compare link below carries the rest.
    #
    # The trailers are addressed to git, not to any reader. Command substitution then drops the
    # blank line that both filters leave behind.
    summary=$(git log -1 --format=%b "$commit" |
        grep -viE '^(co-authored-by|claude-session|signed-off-by):' |
        sed '/^$/q' || true)
    if [ -n "$summary" ]; then
        printf '%s\n\n' "$summary"
    fi
done

if [ -n "$previous" ]; then
    # Set by the workflow. Running this by hand, the remote knows the same thing.
    repository=${GITHUB_REPOSITORY:-$(git remote get-url origin |
        sed -e 's#.*github\.com[:/]##' -e 's#\.git$##')}
    printf -- '---\n\nEvery change in full: https://github.com/%s/compare/%s...%s\n' \
        "$repository" "$previous" "$tag"
fi
