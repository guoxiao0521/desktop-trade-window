---
name: bump-project-version
description: Updates every project version source consistently. Use when the user asks to upgrade, bump, increment, or change the application version.
---

# Bump Project Version

Run the bundled script from the repository root:

```bash
node .cursor/skills/bump-project-version/scripts/bump-version.mjs [version|patch|minor|major]
```

Rules:

1. If the user gives an exact version, pass it without a leading `v` (a leading `v` is also accepted).
2. If the user asks for a patch, minor, or major bump, pass that keyword.
3. If no target is specified, omit the argument; the script defaults to a patch bump.
4. Report the old and new versions and summarize the changed files.
5. Do not commit, push, create a tag, or publish a release unless the user explicitly requests it.
6. If publishing is requested, commit and push the version change before creating and pushing the matching `v<version>` tag.

The script updates and verifies:

- `package.json`
- `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`
- Release-tag examples in `README.md`

If existing version sources disagree, stop and report the mismatch instead of editing files.
