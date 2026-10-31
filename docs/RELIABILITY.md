# Reliability in ReBoneBase 0.2.0

This release improves editable clip operations, native project compatibility, recovery and export publication. It is the first implementation milestone from the application review. The broader additions and UI redesign remain separate work.

## Editable clip windows

Trim, split and resize retain the original sparse animation curves. Keys before and after the visible range remain stored, with their easing and cubic tangents, because they influence interpolation and procedural history. Timeline diamonds, curve handles, and clip-specific key inspectors show the visible range. Moving or copying visible keys remains an ordinary editable operation.

Live layers and contacts continue to run. Springs replay the retained prefix with the current curve values, durations and controller settings, preserving the result of scrubbing to a trimmed frame. Trimming a clip used by another layer does not silently replace that layer's source animation with a newly looped short clip.

Extending a clip holds its previous displayed endpoint. Adding or editing a key in the extension releases the hold through the edited frame for the entire clip, including controllers and scene animation. This may reveal original later motion that was retained by a prior trim. The Studio panel explains when motion is held. Audio and reference clocks continue to advance through the hold; trimming records their original start time.

Props, meshes, expressions and scene actor placements keep sparse independent clip timelines. Events remain attached to the visible frame sequence; extending a hold does not repeat an event from the prior endpoint. Pixel corrections and collision data remain aligned with the edited clip. Explicit mirror/transition generation and **Bake all clips and switch to FK** still produce deliberate baked output. Ordinary trim/split/resize do not implicitly bake.

Visible clips remain limited to 240 frames. Retained source spans are limited to 4,096 frames, and controller-work limits include spring replay. Rejected operations leave the prior document intact. Save/load and undo/redo retain the source-window information.

## Project format and compatibility

The application writes native schema 2. Its compatibility header records the minimum reader schema, writer version and required feature identifiers. The first feature identifier is `editable-clip-windows`.

Schema 1 files migrate in memory when opened, including embedded scene actors. Opening a file does not rewrite it. An explicit save writes schema 2, and normal app saves retain a backup of the previous file. Older builds cannot read schema 2: keep the original or a backup when working across versions.

Loading rejects unsupported schema versions, required features and unknown nested fields instead of allowing them to disappear during a later save. Known legacy `looped` fields are normalized to `loop`; supplying both is rejected. The known optional `source: null` is accepted, while unknown null-valued fields are still rejected. Invalid timing arithmetic returns an error rather than panicking. The 96 MiB project-file limit still applies.

Programmatic native-file callers should use `project::decode`/`load` and `project::encode`/`save`; direct serde deserialization does not perform the file-boundary migration and unknown-field checks.

## Recovery and backups

Recovery writes stage metadata containing an exact-byte fingerprint, commit the validated project snapshot, then promote the metadata. Discovery validates the actual `.rebone` files and can recover valid orphan snapshots with missing, stale or damaged metadata. Only matching metadata supplies an original save path. Legacy metadata is accepted when its identity and project name agree.

Discovery preserves files it cannot load and reports warnings. It skips symlinks and bounds the number and size of reads. The fingerprint detects accidental mismatch or interruption; it is not an authentication mechanism.

Normal app saves serialize and validate the new project before changing backups. The previous file is backed up before replacement. Retention cleanup runs after the main save succeeds; cleanup problems are reported as warnings on a successful save. These protections do not promise survival of every storage-device or power-loss failure.

## Recoverable exports

Batch and game-package exports render and prepare their complete planned output first. Checked rendering propagates a contextual error, including scene actor or 3D character information, instead of producing a successful but incomplete export. Main-canvas, Production and animation-import previews also show render errors.

The publisher validates planned paths, destination conflicts and directory boundaries, then stages replacement bytes and a recovery journal inside the output directory. It checks hard-link support before moving existing files. Planned original files remain available for rollback; unrelated files are left in place. An operating-system file lock prevents two exporters from treating a live transaction as an interrupted one.

If publication fails, rollback restores the prior planned files. If rollback cannot finish, the error identifies the retained recovery directory. The next export to that folder attempts recovery before publishing new output. Recovery refuses to remove conflicting files that were changed independently; move those files to a separate safe location before retrying. Do not delete the recovery directory while originals may still be needed.

This is recoverable multi-file publication. Other processes can briefly observe a mixture of files while an export is being published; the whole directory does not switch in one atomic operation. The destination filesystem must support hard links, so unsupported removable or network volumes are rejected before original files are moved. Windows read-only destination files are also rejected before replacement. If publication finished but cleanup failed, the error explicitly says the new files were published and recovery data remains.

The journal protocol is covered by injected write, install, restore and cleanup failures, interruption recovery, live-lock contention, user-edit conflicts, path validation and symlink tests. Directory syncing is used where the platform supports it; hardware durability is not guaranteed.

## Verification and Windows packages

The GitHub Actions workflow runs formatting, all non-ignored Rust tests, Clippy with warnings denied, and the Python Unreal-helper tests on Linux and Windows. It pins Rust 1.99.1 and exact action revisions. The Windows job builds a portable package after its checks pass and uploads it as a workflow artifact.

`scripts/package.ps1` reads the version and build directory from Cargo, creates `dist/ReBoneBase-<version>/ReBoneBase-<version>.exe`, and includes a SHA-256 checksum, guides, available sample projects and license notices. It respects `CARGO_TARGET_DIR` through Cargo metadata.

Optional tests that invoke a real FFmpeg installation remain ignored by the default workflow. Interactive Windows use and actual Unity/Unreal Editor execution require their own acceptance runs; compiling and running automated tests do not establish those results. Historical coverage in `docs/evidence` predates this milestone.
