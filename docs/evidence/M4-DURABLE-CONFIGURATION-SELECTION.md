# M4 durable configuration-selection evidence

Date: 2026-08-10

Status: portable raw-media selection journal implemented and fault-injection
tested. This does not claim firmware routing, boot activation, board armability,
or physical HIL.

## Implemented boundary

`alumina-storage` now retains a typed `MachineConfiguration` publication through
an explicit two-record transition:

1. `ConfigurationPrepare` durably binds the action, nonzero transaction,
   configuration object kind/digest/length, and exact manifest digest.
2. Core 1 may then activate or clear the independently validated identity.
3. `ConfigurationCommit` must match the prepared bytes exactly and is the sole
   event that changes replayed active state.

Activation prepares may supersede an orphaned prepare but never the active
selection. Clear prepares are admitted only for the exact active publication.
All mutations require `DISARMED_IDLE`-equivalent safety facts. Activation also
resolves an existing typed publication before appending intent. A mount exposes
committed and pending state separately: pending state is inert, while committed
state must still be reopened, streamed, hashed, and validated by both cores at
every boot.

Upload record groups and selection transitions are mutually serialized. A
transition cannot begin or commit during an upload, and an unresolved prepared
transition prevents a new upload. This preserves the publication reader's
contiguous begin/chunk/publish proof while still allowing an interrupted upload
to be explicitly aborted before configuration work resumes.

The records use the existing append-log barriers: complete payload and sync,
complete record-commit block and sync, then replacement alternating anchor and
sync. A device/sync error faults the in-memory media owner; remount selects a
complete anchor and therefore the complete old or new journal state.

## Fault model and checks

The host RAM block device injects a torn partial block or sync failure at every
operation around prepare and commit. Tests prove:

- every activation-prepare cut retains the old active selection;
- every activation-commit cut yields exactly the old selection plus inert
  pending intent, or the complete new selection with no pending intent;
- every clear-commit cut yields exactly the old selection plus inert pending
  clear, or no active selection;
- remount reproduces prepared, committed, and cleared state;
- exact prepare retries consume no additional record;
- missing/wrong-kind publications, unmatched commits and clears, and energized
  mutation attempts reject before append; and
- uploads cannot interleave records with a prepared configuration transition.

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test -p alumina-storage --locked
cargo clippy -p alumina-storage --all-targets --locked -- -D warnings
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
git diff --check
```

This checkpoint is repository-owned `MIT OR Apache-2.0` work and adds no package
dependency. The repository policy admits permissive MIT/Apache-compatible
licenses (including reviewed BSD/ISC/Zlib-style licenses) and excludes GPL,
LGPL, AGPL, SSPL, copied implementation code, and assets from those families.

## Claim boundary and next gate

No target image yet sends configuration commands or consumes this selector at
boot. The next checkpoint adds authenticated firmware request/status routing,
advances core-0 validation incrementally, observes periodic core-1 reports,
orders prepare → core-1 activation → commit, and hands the exact committed
digest to both job actors. Both board packages remain non-armable until their
physical safety/output paths pass HIL.
