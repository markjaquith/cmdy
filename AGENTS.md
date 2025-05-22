 # Codex Assistant Guidance

 This document provides recommended steps for the OpenAI Codex assistant when making changes to the `cmdy` repository.

 ## Workflow

 1. After applying code changes or patches, run:
    ```sh
    cargo check
    cargo fix --allow-dirty
    ```
    - `cargo check` verifies that the code compiles without errors.
    - `cargo fix --allow-dirty` applies automatic fixes (e.g., unused imports) and allows minor edits to your working directory.

 2. Verify functionality and ensure all tests pass:
    ```sh
    cargo test
    ```

 ## Best Practices

 - Keep changes minimal and focused on the user’s request.
 - Maintain coding style consistent with the existing codebase.
 - Use `git status` to review uncommitted changes before concluding a task.
 - Do not add unrelated modifications or fix pre-existing issues outside of the described task scope.
 - Only add code comments when they explain something unusual or complex. Do not narrate all the code.

