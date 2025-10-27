# Blockframe-RS Performance Benchmarks

## October 28, 2025 - BufWriter Optimization

```
═══════════════════════════════════════════════════════════════════════════════
                    BLOCKFRAME-RS PERFORMANCE BENCHMARK
═══════════════════════════════════════════════════════════════════════════════

📊 SYSTEM SPECIFICATIONS:
─────────────────────────────────────────────────────────────────────────────
CPU: 10 physical cores, 16 logical cores
  Frequency: 3700 MHz

RAM: 31.82 GB total

Disk: H: Drive
  Total: 465.75 GB

OS: Windows 11 (AMD64)

═══════════════════════════════════════════════════════════════════════════════
                         BENCHMARK CONFIGURATION
═══════════════════════════════════════════════════════════════════════════════
Runs per condition: 20
Memory constraints: [4GB, 16GB, Unlimited (32GB)]
Total benchmark runs: 60

📁 Test Files:
  big_file.txt: 953.67 MB
  example.txt: 0.00 MB
  Total: 953.67 MB

═══════════════════════════════════════════════════════════════════════════════
🔧 Testing: 4GB Memory Constraint (Abhorrent)
═══════════════════════════════════════════════════════════════════════════════

📈 Statistics for 4GB Memory Constraint (Abhorrent):
─────────────────────────────────────────────────────────────────────────────
  Mean time:        5.606s (±0.198s)
  Fastest:          5.365s
  Slowest:          6.145s
  Mean throughput:  170.83 MB/s
  Estimated 1TB:    1h 42m 6s (1.70 hours)

═══════════════════════════════════════════════════════════════════════════════
🔧 Testing: 16GB Memory Constraint (Moderate)
═══════════════════════════════════════════════════════════════════════════════

📈 Statistics for 16GB Memory Constraint (Moderate):
─────────────────────────────────────────────────────────────────────────────
  Mean time:        5.399s (±0.195s)
  Fastest:          5.110s
  Slowest:          5.827s
  Mean throughput:  177.46 MB/s
  Estimated 1TB:    1h 38m 26s (1.64 hours)

═══════════════════════════════════════════════════════════════════════════════
🔧 Testing: Unlimited Memory (Full 32GB)
═══════════════════════════════════════════════════════════════════════════════

📈 Statistics for Unlimited Memory (Full 32GB):
─────────────────────────────────────────────────────────────────────────────
  Mean time:        5.410s (±0.188s)
  Fastest:          5.092s
  Slowest:          5.771s
  Mean throughput:  176.48 MB/s
  Estimated 1TB:    1h 39m 1s (1.65 hours)

═══════════════════════════════════════════════════════════════════════════════
                          PERFORMANCE COMPARISON
═══════════════════════════════════════════════════════════════════════════════

1. 4GB Memory Constraint (Abhorrent)
   Average: 5.606s | Throughput: 170.83 MB/s

2. 16GB Memory Constraint (Moderate)
   Average: 5.399s | Throughput: 177.46 MB/s
   vs 4GB Memory Constraint (Abhorrent): 1.04x faster | Throughput gain: 3.9%

3. Unlimited Memory (Full 32GB)
   Average: 5.410s | Throughput: 176.48 MB/s
   vs 4GB Memory Constraint (Abhorrent): 1.04x faster | Throughput gain: 3.3%

═══════════════════════════════════════════════════════════════════════════════
                              BENCHMARK COMPLETE
═══════════════════════════════════════════════════════════════════════════════
```

---
