# BlockFrame TODO List

## Generation 2: Core Architecture (Adaptive Multi-Tier)

### Tier 1: Tiny Files (<10MB)

- [x] Implement simple RS(1,3) for files <10MB (commit_tiny implemented)
- [x] Single file storage (no segmentation)
- [x] 3 parity files for full file protection
- [x] Encoding target: <10ms for 5MB files (achievable, not benchmarked yet)
- [x] Recovery: Handle any 2 parity losses (repair_tiny implemented)
- [x] Test with: 1KB, 100KB, 5MB files (tested with example.txt, shakespeare, image)

### Tier 2: Medium Files (10MB-1GB)

- [x] Per-segment RS(1,3) encoding (commit_segmented implemented)
- [x] 300% storage overhead (1 data + 3 parity per segment - higher than planned but functional)
- [x] Memory usage: <35MB during encoding (streams segments, doesn't load full file)
- [ ] Encoding target: <2s for 500MB files (not benchmarked yet)
- [ ] Test segment corruption recovery (recovery function not implemented yet)
- [ ] Test with: 50MB, 500MB files

### Tier 3: Large Files (1GB-10GB) - **CURRENT PRIORITY**

- [x] Implement block-level RS(30,3) encoding
- [x] 30 segments per block, 3 parity chunks per block
- [ ] Fix "segment 21 scenario" - recover from complete segment loss
- [x] Constant 1GB memory usage (uses mmap, doesn't load all into RAM)
- [x] 10% storage overhead target (achieved with 30,3 encoding)
- [ ] Encoding target: <30s for 5GB files (16 threads) - currently 85s on HDD (hardware limited)
- [x] Pre-create all directories before parallel processing
- [x] Test with: 1GB, 5GB, 10GB files (tested 6GB successfully)

### Tier 4: Massive Files (>10GB)

- [ ] Implement hierarchical block parity system
- [ ] Block representatives for cross-block recovery
- [ ] Streaming approach for block parity (avoid loading all blocks)
- [ ] Constant 1GB memory regardless of file size
- [ ] 11-15% storage overhead (configurable)
- [ ] Encoding target: 1TB in <30 minutes
- [ ] Survive 10+ segment losses + 3-10 block losses
- [ ] Test with: 100GB, 1TB files (when possible)

### Cross-Tier Infrastructure

- [x] Automatic tier selection based on file size (implemented in commit())
- [x] Update manifest to include tier metadata (tier field added to manifest)
- [ ] Tier routing in recovery functions
- [x] Unified API across all tiers (single commit() method routes to correct tier)
- [ ] Backward compatibility with Gen 1 archives

---

## Performance Optimizations

### I/O Improvements (Low Priority - Hardware Limited)

- [ ] BufWriter with capacity for segment writes
- [ ] Pre-allocation (set_len) before writes
- [ ] Consider async I/O with tokio (if beneficial)
- [ ] Add antivirus exclusion documentation
- [ ] Benchmark on SSD vs HDD

### Encoding Optimizations

- [ ] Evaluate FFT-based RS libraries (leopard-codec, reed-solomon-32)
- [x] Benchmark reed-solomon-simd vs alternatives (switched from reed-solomon-erasure to reed-solomon-simd)
- [ ] Consider producer-consumer pattern with bounded channels
- [x] Profile RS encoding vs I/O after Tier 3 implementation (profiled: I/O is bottleneck at 74-79s, RS only 1-4s)

### Memory Management

- [x] Ensure reference-based segment processing (Vec<&[u8]>) - implemented to avoid 5GB memory spike
- [ ] Streaming block representative generation for Tier 4
- [ ] Memory profiling for 10GB+ files

---

## Recovery & Health System

### Core Recovery Functions

- [x] Implement segment recovery for Tier 1 (simple parity) - repair_tiny() implemented
- [ ] Implement segment recovery for Tier 2 (per-segment RS)
- [ ] **Implement block-level recovery for Tier 3** (priority)
- [ ] Implement hierarchical recovery for Tier 4
- [ ] Atomic commit process (rollback on failure)
- [ ] Repair interrupted/partial commits

### Health Checks

- [ ] Verify all segments exist and are readable
- [ ] Check parity integrity with merkle tree
- [ ] Detect silent data corruption
- [ ] Report recoverable vs unrecoverable states
- [ ] Automated repair suggestions

### File Store Integration

- [x] List all manifests in archive_directory (FileStore::new() scans directory)
- [x] Parse manifests into File objects (models.rs has File struct)
- [x] File metadata: size, type, tier, creation date (stored in manifest)
- [ ] Check file health status
- [ ] Batch health checks across all files

---

## Testing & Validation

### Unit Tests

- [ ] Test each tier with appropriate file sizes
- [ ] Test tier boundary conditions (9.99MB, 10.01MB, etc)
- [ ] Test segment corruption scenarios
- [ ] Test block corruption scenarios
- [ ] Test complete segment loss (Tier 3+)

### Integration Tests

- [ ] End-to-end commit → verify → corrupt → recover
- [ ] Test all 4 tiers in sequence
- [ ] Interrupted commit recovery
- [ ] Concurrent access safety

### Fault Injection Tests

- [ ] Random segment deletion (1, 2, 3+ segments)
- [ ] Random block deletion (Tier 3+)
- [ ] Bit-flip corruption detection
- [ ] Partial file writes (simulate crashes)
- [ ] Zero data loss validation

### Performance Benchmarks

- [x] Encoding speed across all tiers (Tier 3: 85s for 6GB on HDD)
- [ ] Recovery speed for each fault type
- [ ] Memory usage profiling
- [x] I/O throughput measurement (profiled: segment writes 74-79s, RS encoding 1-4s, parity writes 0.6-3.7s)
- [ ] Compare Gen 1 vs Gen 2 on same hardware

---

## Documentation

### Technical Documentation

- [x] Architecture overview (4 tiers explained) - GENERATION_2_PLAN.md completed
- [ ] Reed-Solomon implementation details
- [ ] Manifest format specification
- [ ] Recovery algorithm explanations
- [x] Memory and performance characteristics (documented in profiling session)

### User Documentation

- [ ] API usage examples for each tier
- [ ] How to choose appropriate tier (if manual)
- [ ] Recovery procedure guide
- [ ] Troubleshooting common issues
- [x] Hardware recommendations (HDD vs SSD performance documented, antivirus impact noted)

### Developer Documentation

- [ ] Code structure walkthrough
- [ ] How to add new tiers
- [ ] Testing guide
- [ ] Contribution guidelines

---

## Generation 3: Future Enhancements (Defer Until Gen 2 Complete)

### Distributed Storage

- [ ] Spread blocks across multiple machines
- [ ] Network-aware placement strategy
- [ ] Replication across availability zones

### Advanced Features

- [ ] Incremental commits (only re-encode changed blocks)
- [ ] Compression layer (LZ4/Zstd before encoding)
- [ ] GPU acceleration (CUDA/OpenCL for Reed-Solomon)
- [ ] Encryption (AES-256-GCM before encoding)
- [ ] Deduplication (block-level across files)

### Network Protocol

- [ ] Stream segments over TCP/gRPC
- [ ] Remote archive access
- [ ] Multi-node coordination

### Cloud Integration

- [ ] S3-compatible API
- [ ] Azure Blob backend
- [ ] Google Cloud Storage backend
- [ ] Hybrid local + cloud storage

---

## Immediate Next Steps (This Week)

1. **Finish Tier 3 implementation** (block-level RS encoding)

   - Fix segment 21 scenario
   - Test with 6GB file on current hardware
   - Validate 10% overhead target

2. **Implement basic recovery** for Tier 3

   - Reconstruct missing segments from block parity
   - Test complete segment loss scenarios

3. **Clean up warnings** in codebase

   - Fix overlapping range patterns in tier matching
   - Remove unused variables in main.rs
   - Address dead code warnings

4. **Document findings**
   - [x] Write up I/O bottleneck analysis (completed: HDD random I/O is bottleneck, not RS encoding)
   - [x] Hardware requirements guide (HDD: 88 MB/s sequential but 1.5 MB/s random; SSD recommended)
   - [x] Performance expectations per drive type (HDD: ~85s, Enterprise HDD: ~55s, SSD: ~23s for 6GB)

---

## Long-Term Vision (3-6 Months)

- [ ] All 4 tiers production-ready
- [ ] Comprehensive test coverage (>80%)
- [ ] Benchmark comparison vs S3/Azure
- [ ] Case study: TB-scale dataset handling
- [ ] Portfolio presentation materials
- [ ] Blog post: "Building a TB-Scale Storage System in Rust"

---

## Notes

**Current Status:**

- Tier 3 architecture fully implemented (encoding complete)
- Parallel block processing working with Rayon
- I/O bottleneck identified and profiled (HDD random I/O: 1.5 MB/s, segment writes: 74-79s per block)
- RS encoding optimized with reed-solomon-simd (only 1-4s per block)
- Memory optimization complete (reference-based segments, Vec<&[u8]>)
- Automatic tier selection implemented
- Manifest includes tier metadata
- FileStore can parse and list files
- Tier 1 recovery (repair_tiny) implemented

**Blockers:**

- Hardware limitation (HDD not suitable for 180-file workload)
- Tier 2 recovery functions not yet implemented
- Tier 3 recovery functions not yet implemented
- Tier 4 encoding not yet implemented

**Decisions Made:**

- Defer Tier 4 until Tier 3 complete
- Accept current I/O performance on HDD (code is fine)
- Focus on correctness and recovery before further optimization
- Use RS(1,3) instead of RS(6,3) for Tiers 1 and 2 (simpler, less overhead)
- No AI-generated code (learning exercise)
