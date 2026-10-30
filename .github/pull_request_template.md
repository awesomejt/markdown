# Phase Pull Request

## Delivery phase

Phase ID / parent milestone:
Host / branch:
Prerequisite merged PRs:

## Result

Describe the concrete behavior and acceptance criteria met. State exclusions and
compatibility or capability changes.

## Validation

Record commands, actual host/tool versions, outcomes and evidence. Identify
unrun required checks; open the PR only once required checks pass, because
creating it starts the review.

## Risks and handoff

Record residual risks, native-platform limits and resume instructions if needed.

## Review

Creating this PR starts the Grok review, unless the branch is `review/*` or
`patch/*`, which Grok ignores. Request a later review with a **separate PR
conversation comment** that is exactly `/grok review`; at most three reviews
per PR. Record the reviewed head SHA, the review link and the disposition of
findings here or in PR discussion. A `patch/` PR instead says why it needs no
review and merges on green CI (D-033).
