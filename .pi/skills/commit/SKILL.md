---
name: commit
description: "Creates a git commit using Conventional Commits format. Use whenever the user asks to commit changes, stage and commit work, or write a commit message. Analyzes the staged/unstaged diff, groups related changes, and writes a type(scope): description message."
---

# Commit

Create a git commit following the [Conventional Commits](https://www.conventionalcommits.org/) specification.

## Workflow

1. **Inspect the repository state.** Run these to understand what changed:
   ```bash
   git status
   git diff            # unstaged changes
   git diff --staged   # already staged changes
   git log -n 5 --oneline   # match existing commit style
   ```

2. **Stage the right changes.** Unless the user already staged specific files or said
   otherwise, stage everything that is relevant to a single logical change:
   ```bash
   git add <files>
   ```
   Avoid `git add -A` when unrelated changes are present; prefer one commit per
   logical change and ask the user how to split when in doubt.

3. **Determine the type.** Pick the most accurate type from the list below based on
   the *primary* effect of the diff, not every file touched:

   | Type | When to use |
   |------|-------------|
   | `feat` | A new feature for the user |
   | `fix` | A bug fix |
   | `docs` | Documentation only changes |
   | `style` | Formatting, whitespace, semicolons, etc. (no code logic change) |
   | `refactor` | Code change that neither fixes a bug nor adds a feature |
   | `perf` | Code change that improves performance |
   | `test` | Adding or correcting tests |
   | `build` | Changes to build system or external dependencies (cargo, npm, etc.) |
   | `ci` | Changes to CI configuration files and scripts |
   | `chore` | Maintenance tasks, tooling, configs, gitignore |
   | `revert` | Reverting a previous commit |

4. **Determine the scope (optional).** Use a short lowercase scope naming the part of
   the project affected (e.g. an module, crate, or component name). Omit the scope
   when the change is broad or cross-cutting.

5. **Write the subject line.** Rules:
   - Format: `type(scope): <description>` or `type: <description>`
   - Lowercase imperative mood ("add", "fix", "refactor"), not past tense
   - No trailing period
   - Keep it under 72 characters
   - Specific and meaningful, not generic ("fix bug" is bad)

6. **Write the body (optional).** Add a body when the change needs explanation:
   - Leave one blank line after the subject
   - Wrap lines around 72 characters
   - Explain *what* and *why*, not *how* (the diff shows how)
   - Use bullet points with `-` for multiple points

7. **Write the footer (optional).** Use a footer to reference issues or note breaking
   changes:
   - Breaking change: `BREAKING CHANGE: <description>` or append `!` after the
     type/scope, e.g. `feat(api)!: remove deprecated endpoint`
   - Issue refs: `Closes #123`, `Refs #456`

8. **Create the commit.** Pass the message with `-m` flags (one per paragraph):
   ```bash
   git commit -m "type(scope): subject" -m "optional body" -m "optional footer"
   ```
   Never use `--no-verify` or bypass hooks unless the user explicitly asks.

9. **Confirm.** Run `git log -n 1 --stat` (or `git show HEAD`) and report the final
   commit hash and message to the user.

## Reference Format

```
<type>(<scope>): <short description in imperative mood>

<optional body explaining what and why, wrapped at ~72 chars>

<optional footer like BREAKING CHANGE: ... or Closes #123>
```

## Examples

```
feat(auth): validate refresh token expiry before refresh

The refresh flow previously requested a new token without checking
whether the current refresh token was still valid, causing repeated
401s. Add an explicit expiry check and return early.

Closes #142
```

```
fix(parser): handle empty input without panic

Guard against zero-length input in the top-level parse entry point.
```

```
docs: expand build steps in README
```

```
refactor(ecs)!: replace custom allocator with slab

BREAKING CHANGE: EntityAllocator trait removed; use SlabAllocator.
```

## Rules

- One logical change per commit. If the diff spans unrelated areas, suggest
  splitting into multiple commits and ask the user how to proceed.
- If there is nothing to commit (clean tree), say so instead of forcing a commit.
- Never fabricate file contents or changes; always read the actual diff first.
- Do not amend, rebase, or rewrite history unless the user explicitly asks.
