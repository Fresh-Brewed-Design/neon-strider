# Release notes for the next version

Each PR adds its own file here instead of editing CHANGELOG.md, so PRs never
conflict with each other or with the release commit.

- Name: `<kind>-<slug>.md`, where kind is `new`, `changed` or `fixed`
  (e.g. `fixed-engage-timeout.md`).
- Contents: one or more `- ` bullet lines, written for the user.

When a PR merges into main, `.github/workflows/auto-release.yml` folds every
file here into CHANGELOG.md under the new version and deletes them.
