# Performance Optimization Plan for Aperture Library Exporter

## Problem Statement

The export program is currently single-threaded and only uses 1 CPU (100% of one core) on an 8-core system. Performance is slow when processing large Aperture Libraries.

## Current Bottlenecks

Based on code analysis, the main performance bottlenecks are:

1. **Loading versions/masters** (`load_versions_items` in library.rs)
   - Sequential file reading from disk (thousands of `.apversion` and `.apmaster` files)
   - Each file is opened, parsed, and processed one at a time
   - Lines 505-558: Single-threaded loop through file list

2. **Export job processing** (`exporter.rs` lines 378-399)
   - Each master + versions exported sequentially
   - File copying and XMP generation done one job at a time
   - Could easily parallelize since jobs are independent

3. **Building export jobs** (`build_export_jobs` lines 252-329)
   - Nested loops iterating over masters and versions
   - Single-threaded HashMap lookups

## Proposed Solutions

### Quick Win #1: Parallelize Export Jobs (EASIEST)

**Impact**: High (could see 4-8x speedup on export)  
**Complexity**: Very low (2 lines of code)  
**Risk**: Low

Add the `rayon` crate for parallel iterators and change the export loop.

**Changes needed:**

1. Add dependency to `Cargo.toml`:
```toml
rayon = "1.8"
```

2. In `src/bin/dumper/exporter.rs`, add import:
```rust
use rayon::prelude::*;
```

3. Change line 378 from:
```rust
for job in &jobs {
```
to:
```rust
jobs.par_iter().for_each(|job| {
```

4. Remove the `for` loop closing brace and replace with closing `});`

5. Handle the println! statements (might need to use a mutex or remove them for parallel execution)

**Estimated effort**: 15 minutes  
**Estimated speedup**: 4-8x for the export phase

### Quick Win #2: Parallelize Version/Master Loading

**Impact**: Medium-High (faster initial library scan)  
**Complexity**: Low-Medium  
**Risk**: Low

Use `rayon` to parallelize the file loading in `load_versions_items`.

**Changes needed:**

1. In `src/library.rs`, import rayon:
```rust
use rayon::prelude::*;
```

2. Change the loading loop (around line 523):
```rust
// OLD:
for file in file_list {
    // ... processing ...
}

// NEW:
file_list.par_iter().for_each(|file| {
    // ... processing ...
});
```

3. **Important**: Make the progress bar thread-safe with `Arc<Mutex<ProgressBar>>`
4. Make the auditor thread-safe if needed

**Estimated effort**: 30-60 minutes  
**Estimated speedup**: 2-4x for library loading phase

### Quick Win #3: Use Buffered I/O

**Impact**: Low-Medium (10-30% improvement)  
**Complexity**: Very low  
**Risk**: None

Ensure file operations use buffering where appropriate.

**Changes needed:**

Check if `fs::copy()` and file reads are already buffered (they usually are in Rust), but could add explicit `BufReader`/`BufWriter` if needed.

**Estimated effort**: 15 minutes  
**Estimated speedup**: 10-30% for file I/O

## Recommended Implementation Order

### Phase 1: Easiest Wins (30 minutes)
1. ✅ Add `rayon` dependency to Cargo.toml
2. ✅ Parallelize export jobs loop
3. ✅ Test with small library
4. ✅ Test with large library

### Phase 2: Medium Effort (1-2 hours)
1. ✅ Parallelize version/master loading
2. ✅ Make progress bar thread-safe
3. ✅ Test thoroughly

### Phase 3: Optional Refinements
1. Profile to identify remaining bottlenecks
2. Consider async I/O for network-mounted libraries
3. Add parallel processing for other operations

## Implementation Guide

### Step-by-Step: Parallelize Export Jobs

```bash
# 1. Edit Cargo.toml
# Add under [dependencies]:
rayon = "1.8"

# 2. Edit src/bin/dumper/exporter.rs
# Add at top with other imports:
use rayon::prelude::*;

# 3. Find the export loop (around line 378):
# Change from:
for job in &jobs {
    println!("Exporting master {} and {} versions...", 
             job.master_uuid, job.version_uuids.len());
    if !args.dryrun {
        if let Err(e) = export_job_files(...) {
            eprintln!("Error: {}", e);
        }
    }
}

# To:
use std::sync::Mutex;
let error_count = Mutex::new(0);

jobs.par_iter().for_each(|job| {
    // Note: println! in parallel code can interleave, 
    // so you might want to remove or use a proper logging system
    if !args.dryrun {
        if let Err(e) = export_job_files(
            job,
            out_dir,
            &cache,
            &library_abs,
            &flat_keyword_map,
            &hierarchical_keyword_map,
        ) {
            eprintln!("Error exporting {}: {}", job.master_uuid, e);
            *error_count.lock().unwrap() += 1;
        }
    }
});

let errors = error_count.into_inner().unwrap();
if errors > 0 {
    eprintln!("Export completed with {} errors", errors);
}

# 4. Build and test:
cargo build --release
cargo run --release --bin export -- ~/Pictures/MyLibrary.aplibrary
```

## Expected Performance Gains

### Before (Single-threaded):
- Library loading: ~40 minutes (network mount)
- Export processing: ~10 minutes
- **Total: ~50 minutes**

### After Phase 1 (Parallel export):
- Library loading: ~40 minutes (unchanged, uses cache)
- Export processing: ~2 minutes (5x speedup)
- **Total: ~42 minutes** (16% improvement)

### After Phase 2 (Parallel loading + export):
- Library loading: ~10 minutes (4x speedup)
- Export processing: ~2 minutes (already optimized)
- **Total: ~12 minutes** (75% improvement, 4x faster overall)

## Trade-offs

**Advantages:**
- ✅ Much faster processing (4-8x potential speedup)
- ✅ Better CPU utilization (use all 8 cores)
- ✅ Minimal code changes required
- ✅ `rayon` is production-ready and widely used

**Disadvantages:**
- ⚠️ Progress output will be less smooth (parallel println!)
- ⚠️ Slightly higher memory usage (parallel processing overhead)
- ⚠️ Need to ensure thread-safety (but Rust helps prevent bugs)
- ⚠️ Debugging parallel code is harder

## Safety Considerations

Rust's ownership system prevents most parallel programming bugs, but be aware of:

1. **Shared mutable state**: Use `Mutex` or `RwLock` for shared counters/progress
2. **println! interleaving**: Output can mix between threads
3. **File system**: Multiple threads writing to same directory is fine
4. **Cache**: HashMap is read-only during export, so it's safe

## Alternative: Async I/O (More Complex)

If the library is on a network mount and I/O is the bottleneck, consider using `tokio` for async I/O:

- More complex to implement (~1 week effort)
- Better for I/O-bound workloads (network-mounted libraries)
- Could combine with parallel processing for best results

## Testing Plan

1. **Small test library**: Verify correctness with known output
2. **Large library (network)**: Measure performance improvement
3. **Large library (local SSD)**: Verify works well on fast storage
4. **Verify output**: Ensure XMP files are correctly generated
5. **Check for race conditions**: Run multiple times, compare outputs

## Monitoring Performance

```bash
# Monitor CPU usage during export:
watch -n 1 'ps aux | grep export'

# Or use htop:
htop

# Time the full export:
time cargo run --release --bin export -- ~/Pictures/Library.aplibrary

# Profile to find remaining bottlenecks:
cargo install flamegraph
cargo flamegraph --bin export -- ~/Pictures/Library.aplibrary
```

## Next Steps

1. Review this plan
2. Backup your library (just in case)
3. Implement Phase 1 (30 minutes)
4. Test and measure performance
5. If satisfied, stop here or continue to Phase 2
6. Consider contributing the improvements back to upstream

## References

- Rayon documentation: https://docs.rs/rayon/
- Rust parallelism book: https://rust-lang.github.io/book/ch16-00-concurrency.html
- Performance profiling: https://nnethercote.github.io/perf-book/
