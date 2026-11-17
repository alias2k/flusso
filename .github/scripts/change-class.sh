#!/usr/bin/env bash
# Decide whether this job's run is a docs-only pull request (issue #402); run by
# .github/actions/change-class right after a `fetch-depth: 2` checkout.
#
# Env: EVENT, JOB, BASE_REF, DEFAULT_BRANCH, GITHUB_OUTPUT, GITHUB_STEP_SUMMARY, RUNNER_TEMP.
#
# The rule is the BASE's .github/scripts/change_class.py (`git show HEAD^1:…`), never the PR's own
# copy, so a PR can't loosen the rule that judges it. Anything that can't be decided is a full
# run, written here: not a pull request, no merge commit, no classifier on the base, no diff, no
# Python, or a classifier that fails or writes no verdict.
set -uo pipefail

full() {
  printf 'docs_only=false\ncargo=true\n' >>"$GITHUB_OUTPUT"
  printf '**Full suite**: %s.\n' "$1" >>"$GITHUB_STEP_SUMMARY"
  echo "full: $1" >&2
  exit 0
}

[ "${EVENT:-}" = pull_request ] || full "a \`${EVENT:-unknown}\` run checks everything"
# On a pull request the checkout is the merge commit: parent 1 the base, parent 2 the PR head.
git rev-parse --verify --quiet 'HEAD^2' >/dev/null || full "the checkout is not a pull request's merge commit"

tmp=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/change-class.XXXXXX") || full "no temporary directory"
git show 'HEAD^1:.github/scripts/change_class.py' >"$tmp/change_class.py" 2>/dev/null \
  || full "the base has no classifier (\`.github/scripts/change_class.py\`)"
git diff --name-status -z -M 'HEAD^1' HEAD >"$tmp/changes" || full "the diff could not be taken"
# Windows runners may name it `python` only.
python=$(command -v python3 || command -v python) || full "no Python on this runner"

if ! "$python" -I "$tmp/change_class.py" --event "$EVENT" --job "${JOB:-}" \
  --base-ref "${BASE_REF:-}" --default-branch "${DEFAULT_BRANCH:-}" \
  --changes "$tmp/changes" --root . --output "$tmp/output" --summary "$tmp/summary"; then
  echo "::warning::the base's change_class.py failed; running everything"
  full "the base's classifier failed"
fi
grep -q '^docs_only=' "$tmp/output" 2>/dev/null || full "the base's classifier gave no verdict"
cat "$tmp/output" >>"$GITHUB_OUTPUT"
cat "$tmp/summary" >>"$GITHUB_STEP_SUMMARY" 2>/dev/null || true
