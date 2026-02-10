# Codex Assistant Guidance

This document provides recommended steps for the OpenAI Codex assistant when making changes to the `cmdy` repository.

## Workflow

1. After applying code changes or patches, run:

   ```sh
   cargo check
   cargo fix --allow-dirty
   cargo fmt
   cargo clippy -- -D warnings
   ```

   - `cargo check` verifies that the code compiles without errors.
   - `cargo fix --allow-dirty` applies automatic fixes (e.g., unused imports) and allows minor edits to your working directory.
   - `cargo fmt` ensures consistent code formatting.
   - `cargo clippy -- -D warnings` checks for common mistakes and ensures no clippy warnings.

2. Verify functionality and ensure all tests pass:

   ```sh
   cargo test                    # Unit tests
   cargo test --test integration # Integration tests
   ```

3. Before pushing changes, run the pre-push checks:
   ```sh
   ./scripts/pushable
   ```

## Handling Dependabot PRs

When processing dependabot pull requests:

1. Check all open dependabot PRs:

   ```sh
   gh pr list --author "app/dependabot" --state open
   ```

2. For each PR, checkout and run validation:

   ```sh
   gh pr checkout <PR_NUMBER>
   ./scripts/pushable
   ```

3. Common issues and fixes:
   - **Clippy warnings**: Fix any `map().unwrap_or_else()` warnings by replacing with `map_or_else()`
   - **Integration test failures**: Ensure the program handles edge cases gracefully (e.g., missing directories should exit with code 0)
   - **Merge conflicts**: After merging one PR, rebase others on main to resolve conflicts

4. If fixes are needed:
   ```sh
   # Make necessary fixes
   cargo fmt
   git add -A
   git commit -m "fix: <description of fix>"
   git push
   ```

## Creating Releases

The project uses `cargo-release` for version management. Follow these steps:

1. Ensure all tests pass and CI is green:

   ```sh
   cargo test
   ./scripts/pushable
   ```

2. Determine version bump type:
   - **patch** (0.1.5 → 0.1.6): Bug fixes, dependency updates
   - **minor** (0.1.5 → 0.2.0): New features, backward-compatible changes
   - **major** (0.1.5 → 1.0.0): Breaking changes

3. Create the release:

   ```sh
   # For patch release (most common)
   cargo release patch --no-publish --execute

   # For minor release
   cargo release minor --no-publish --execute

   # For major release
   cargo release major --no-publish --execute
   ```

4. The release process will:
   - Update version in Cargo.toml and Cargo.lock
   - Create a release commit: "chore: release X.Y.Z"
   - Create a git tag: "vX.Y.Z"
   - Push commit and tag to GitHub
   - Trigger GitHub Actions to publish to crates.io

5. If a release fails CI, rollback and retry:

   ```sh
   # Delete the tag locally and remotely
   git tag -d v0.1.X
   git push origin :v0.1.X

   # Fix the issue, then re-release
   cargo release patch --no-publish --execute
   ```

## Best Practices

- Keep changes minimal and focused on the user's request.
- Maintain coding style consistent with the existing codebase.
- Use `git status` to review uncommitted changes before concluding a task.
- Do not add unrelated modifications or fix pre-existing issues outside of the described task scope.
- Only add code comments when they explain something unusual or complex. Do not narrate all the code.
- Always run `cargo fmt` before committing to ensure consistent formatting.
- Ensure CI passes before merging any changes.
