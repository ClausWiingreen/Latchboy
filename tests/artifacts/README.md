# Test Artifacts Evidence Contract

This directory stores **non-copyrighted** milestone evidence artifacts used for closure reviews.

## Required evidence record fields (per milestone)

For each milestone closure submission, include a short markdown section with:

1. **Milestone identifier** (for example: `Milestone 4`).
2. **Manifest delta** referencing the required `tests/rom_manifest.toml` entries that were added or changed.
3. **Pass/fail table** listing required ROM IDs, execution outcome, and run date.
4. **Hash summary links** to committed metadata-only outputs (for example `milestone4-smoke-summary.json`).
5. **CI evidence links** (workflow/job/check names) that enforce the same gate.

## Allowed artifact content

- ✅ JSON/Markdown metadata.
- ✅ Deterministic hashes and checkpoint indices.
- ✅ Log snippets that do not include copyrighted ROM payloads.

## Forbidden artifact content

- ❌ Raw ROM files.
- ❌ Copyrighted frame/image/video dumps from commercial games.
- ❌ Binary captures that embed copyrighted ROM data.

## Status marker vocabulary

Use these exact terms in milestone review notes to avoid ambiguity:

- **Implemented**: behavior exists in code.
- **Documented**: behavior and closure criteria are captured in repository docs.
- **Gated**: CI/tests enforce required pass conditions.
