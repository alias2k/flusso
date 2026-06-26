---
description: Discuss a problem, then open a well-described GitHub issue (assigned + labeled)
---

The user wants to open a new GitHub issue about: **$ARGUMENTS**

Your job is to produce a high-quality issue whose value is the **problem description**, not a
solution. Follow this flow:

## 1. Understand the problem (do this first — do NOT create the issue yet)

- Investigate the problem in the codebase as needed so your questions are informed, not generic.
- Ask the user clarifying questions about the **problem itself** to make the description as
  detailed and precise as possible: what triggers it, scope, who/what is affected, expected vs
  actual behavior, constraints, edge cases, why it matters. Prefer the `AskUserQuestion` tool for
  crisp choices; use plain follow-ups for open-ended detail.
- Keep digging until the problem is unambiguous. Resolve every "it depends" before drafting.

## 2. Draft the issue

- **The description must NOT contain the solution.** It MAY include a "Possible solutions"
  section listing *options* under consideration — clearly framed as possibilities, not a chosen
  plan.
- Lead with the problem. Suggested structure:
  - **Problem** — what's wrong / what's missing, concretely.
  - **Context** — where it shows up, relevant code (`file:line`), how to reproduce if it's a bug.
  - **Impact** — why it matters / who it affects.
  - **Possible solutions** (optional) — candidate approaches, each with a one-line trade-off.
- Show the drafted title + body to the user and get a thumbs-up before creating.

## 3. Create it

Once approved, create the issue with `gh`:

- **Assign to the user**: `--assignee @me`
- **Label**: pick the single best-fitting default GitHub label for this repo — usually `bug` or
  `enhancement`, but choose `documentation`, `question`, etc. when it fits better. If unsure
  between two, ask.
- Command shape:
  ```sh
  gh issue create --title "<title>" --body "<body>" --assignee @me --label <label>
  ```
- Report back the created issue URL.
