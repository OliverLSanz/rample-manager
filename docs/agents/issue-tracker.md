# Issue tracker: GitHub

Issues and specs live in `OliverLSanz/rample-preview` on GitHub.
Use the `gh` CLI from this clone.

## Operations

- Confirm the target before creating issues:
  `gh repo view --json nameWithOwner`.
  File issues in `OliverLSanz/rample-preview`.
- Create: `gh issue create --title "..." --body-file <path>`.
  Write multiline bodies to a temporary file with actual newlines.
- Read: `gh issue view <number> --json number,title,body,labels,comments`.
- List: `gh issue list --state open --json number,title,body,labels,assignees`.
  Add label filters as needed.
- Comment: `gh issue comment <number> --body-file <path>`.
- Label: `gh issue edit <number> --add-label "..."` or
  `--remove-label "..."`.
- Close: post the explanation in a separate comment, then run
  `gh issue close <number>`.

## Relationships

Use native sub-issues and dependencies:

- Create a child: `gh issue create --parent <parent> ...`.
- Link an existing child:
  `gh issue edit <parent> --add-sub-issue <child>`.
- Add a blocker:
  `gh issue edit <child> --add-blocked-by <blocker>`.
- Count open blockers:
  `gh api repos/OliverLSanz/rample-preview/issues/<number> --jq .issue_dependencies_summary.blocked_by`.

If native relationships are unavailable, put `Part of #<parent>` and
`Blocked by: #<number>` at the top of the child body, and link children
from a task list in the parent. A ticket is unblocked when every blocker
is closed.

## Skill terminology

“Publish to the issue tracker” means create a GitHub issue.
“Fetch the relevant ticket” means read the corresponding GitHub issue.

## Pull requests as a triage surface

**PRs as a request surface: no.**
