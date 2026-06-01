# Version Filename Strategy Design

## Status

- Decision made: use Option 4 as default, with Option 3 fallback.
- Implementation status: not implemented yet.
- This document is intended to be implementation-grade guidance.

## Problem Statement

Current exported version filenames are machine-oriented and hard to read:

- Current format: `<master_uuid>_version_<version_uuid>.<ext>`
- This is unambiguous but visually noisy and difficult to work with in normal photo workflows.

We want names that are:

- Human-readable first
- Deterministic and stable
- Collision-safe
- Backward-compatible with current export structure and sidecar behavior

## Chosen Strategy

### Default (Option 4)

Use Version name when available:

- Base output pattern: `<master_stem>__<sanitized_version_name>.<ext>`
- Example: `PICT0019__BW-High-Contrast.jpg`

### Fallback (Option 3)

If Version name is missing or sanitizes to empty, use short UUID-derived token:

- Fallback pattern: `<master_stem>__v-<short_uuid_token>.<ext>`
- Example: `PICT0019__v-7krjr0tq.jpg`

This guarantees a usable filename even when metadata is incomplete.

## Scope

This design only changes exported version image and sidecar filenames.

This design does not change:

- Master image filenames
- Directory layout
- How version image source paths are discovered
- XMP content generation (except optional metadata additions described below)

## Exact Implementation Targets

Primary implementation file:

- `src/bin/dumper/exporter.rs`

Related types/inputs:

- `Version` fields from `src/version.rs`: `name`, `file_name`, `version_number`, `master_uuid`
- `Master` context from export job in `src/bin/dumper/exporter.rs`

## Required Behavior

1. Keep master filename behavior unchanged.
2. Generate version filenames using default Option 4 and fallback Option 3.
3. Ensure deterministic output order for versions per master.
4. Prevent filename collisions within each master output directory.
5. Keep sidecar naming consistent by continuing to use `dest.with_extension("xmp")`.

## Deterministic Ordering Requirement

Current version collection iterates a `HashMap`, which is not deterministic.

Before naming versions for a master, collect matching versions and sort them by:

1. `version.version_number` ascending (None sorts after Some)
2. `version.create_date` ascending (None sorts after Some)
3. `version.uuid` lexicographically as final tie-breaker

If any date field comparison is awkward due to type handling, it is acceptable to skip date and use:

1. `version.version_number`
2. `version.uuid`

The key requirement is stable order across runs for same input data.

## Filename Construction Rules

### Step 1: Determine extension

Use existing logic based on version file metadata:

- Prefer extension from `version.file_name`
- If no extension exists, use empty extension (same as current behavior)

### Step 2: Determine master stem

Use exported master filename stem from `job.master_filename`:

- `master_stem = file_stem(job.master_filename)`
- If empty, use `job.master_uuid`

### Step 3: Build readable label

Candidate label source priority:

1. `version.name` (trimmed)
2. If empty, fallback path (short UUID token)

Sanitize label with a dedicated helper:

- Remove or replace path separators (`/` and `\\`)
- Remove Windows-illegal characters: `< > : " / \\ | ? *`
- Remove control characters and NUL
- Collapse whitespace runs to single `-`
- Collapse repeated `-`
- Trim leading/trailing `.`, `-`, and spaces
- Enforce max label length (recommended 48 chars)

If sanitized label is empty, use fallback token.

### Step 4: Build fallback token (Option 3)

Derive deterministic token from `version_uuid` with only safe characters:

- Keep ASCII alphanumeric only
- Lowercase
- Take first 8 chars
- If fewer than 8 chars remain, use all available chars
- If nothing remains, use `unknown`

Fallback label becomes `v-<token>`.

### Step 5: Assemble filename

Assemble base filename:

- `<master_stem>__<label_or_fallback>`
- append extension if present

### Step 6: Collision handling

Track used filenames per job (per output directory):

- Initialize with master filename and all already-assigned version filenames
- If collision occurs, append `__u-<token>` (token from version UUID)
- If still collides, append numeric suffix `__n2`, `__n3`, etc.

Collision handling must be deterministic.

## Suggested Helper Functions (in exporter.rs)

Add small pure helpers for testability:

- `fn sanitize_filename_component(input: &str, max_len: usize) -> String`
- `fn short_uuid_token(version_uuid: &str) -> String`
- `fn master_stem(master_filename: &str, master_uuid: &str) -> String`
- `fn make_version_filename(master_stem: &str, version_name: Option<&str>, version_uuid: &str, ext: Option<&str>) -> String`
- `fn dedupe_filename(candidate: String, used: &mut HashSet<String>, token: &str) -> String`

Then create one orchestration helper:

- `fn build_version_filenames_for_master(...) -> Vec<String>`

This keeps `build_export_jobs` readable and easy to review.

## Integration Plan

### In `build_export_jobs`

Replace current hardcoded naming block:

- Current: `format!("{}_version_{}{}", master_uuid, version_uuid, ext)`
- New: use helper pipeline described above

Implementation sequence:

1. Gather versions for one master into a temporary vector containing:
   - `version_uuid`
   - source path
   - extension
   - `version.name`
   - `version.version_number` (for sort)
2. Sort temporary vector deterministically.
3. Generate filenames using Option 4 default + Option 3 fallback.
4. Apply collision handling.
5. Push sorted paths, UUIDs, filenames into job vectors in matched order.

### In `export_job_files`

No behavioral changes needed except consuming new names (already done today).

## Optional Metadata Additions (Recommended)

To improve traceability, add these custom fields to version XMP if desired:

- `aplib:ExportVersionFilename`
- `aplib:ExportNamingStrategy` with value `name_or_short_uuid_fallback`

This is optional but useful for debugging and migration audits.

## Test Plan

Add tests in `src/bin/dumper/exporter.rs` under existing `#[cfg(test)]` module.

### Unit Tests for Helpers

1. `sanitize_filename_component`:
- normal text preserved
- illegal chars replaced/removed
- repeated spaces collapsed
- empty result handling
- truncation to max length

2. `short_uuid_token`:
- mixed symbols stripped
- lowercase conversion
- short input
- empty/invalid input

3. `make_version_filename`:
- uses version name when present
- falls back when name missing
- extension present vs missing

4. `dedupe_filename`:
- no collision case
- collision with master filename
- repeated collisions requiring numeric suffix

### Integration Tests

Create small in-memory fixture (or minimal mock data) to verify:

1. Two versions with unique names:
- output names use readable labels

2. Two versions with same name:
- second gets deterministic dedupe suffix

3. Missing version names:
- both use fallback tokens and remain unique

4. Stable ordering:
- same input map iteration order variations still produce same output ordering and names

## Acceptance Criteria

Implementation is complete when all are true:

1. Exported version filenames are human-readable in the common case.
2. Missing/unusable names still produce deterministic unique filenames.
3. Repeated exports of same input produce identical filenames.
4. Sidecars remain paired with their corresponding image filenames.
5. Existing export flow and throttling behavior remain unchanged.
6. New/updated tests pass.

## Rollout Notes

Recommended rollout:

1. Implement without CLI flags first (new strategy as default).
2. Validate on test library and one real subset export.
3. If needed later, add optional CLI strategy switch for backward compatibility with old naming.

## Minimal Pseudocode

```text
for each master in sorted masters:
  collect versions for this master
  sort versions deterministically
  used_names = { master_filename }

  for each version in sorted versions:
    ext = extension(version.file_name)
    stem = stem(master_filename) or master_uuid

    if sanitized(version.name) is not empty:
      label = sanitized(version.name)
    else:
      label = "v-" + short_uuid_token(version.uuid)

    candidate = stem + "__" + label + ext
    final_name = dedupe(candidate, used_names, short_uuid_token(version.uuid))
    used_names.add(final_name)

    store final_name in ExportJob.version_filenames
```
