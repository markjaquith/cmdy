# Release Recovery Guide

If a release fails CI checks on GitHub:

## Option 1: Fix and Re-release (Recommended)
1. Fix the issue that caused CI to fail
2. Delete the failed tag locally and remotely:
   ```sh
   git tag -d v0.1.X
   git push origin :v0.1.X
   ```
3. Create a new release with the same version:
   ```sh
   cargo release patch --no-publish --execute
   git push --follow-tags
   ```

## Option 2: Skip to Next Version
1. Fix the issue that caused CI to fail
2. Create a new release with the next version:
   ```sh
   cargo release patch --no-publish --execute
   git push --follow-tags
   ```
3. The failed version number will be skipped

## Prevention
Consider running CI checks locally before releasing:
```sh
cargo fmt -- --check
cargo clippy -- -D warnings
cargo build --release
cargo test
```

Or create a pre-release script that runs these checks.