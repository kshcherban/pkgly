# S3 performance and robustness plan

This checklist records the S3 performance review and its implementation status.

## Engineering work

- [x] Eliminate duplicate Docker blob downloads. Preserve verified digests and deliver verified
  content from the same read, including large objects that use bounded temporary files.
- [x] Replace quadratic S3 append read/rewrite behavior for Docker uploads with bounded local
  staging and one guarded upload during finalization. Preserve the generic storage append
  contract for other repository types.
- [x] Bound total memory and temporary storage consumption. Stream large S3 writes and cache
  reads, apply shared body and temporary-file budgets, and spool incoming upload chunks in
  bounded buffers.
- [x] Guard cache publication against concurrent mutations. Use per-object generations and
  per-key miss coordination so stale in-flight reads cannot replace newer cache entries.
- [x] Reduce metadata round trips and listing contention. Memoize safe ancestor probes, coordinate
  cold manifest loads per repository, and paginate manifest listings directly from S3.
- [x] Add Docker accounting reconciliation. Reconcile PostgreSQL rows with storage inventory,
  backfill missing manifest graphs, correct size drift, remove missing rows, and use revisions
  to avoid overwriting concurrent writes.
- [x] Document S3 cache integrity behavior, Docker upload staging, resource limits, and the
  one-writer-per-S3-repository deployment constraint.

## Validation work

- [x] Add regression coverage for cache publication races, concurrent cache misses, staged
  appends, and accounting backfill detection.
- [x] Run the workspace Rust test suite, S3 storage tests, Clippy, formatting checks, and the
  real MinIO S3 integration suite.
- [ ] Benchmark cold and warm pulls plus chunked pushes at 1, 16, and 64 concurrent clients,
  using 1 MiB, 64 MiB, and 1 GiB artifacts. Record p95 time to first byte, throughput, peak
  memory, S3 request counts, and transferred bytes per delivered byte.
