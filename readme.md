# BlockFrame [![Release](https://github.com/crushr3sist/blockframe-rs/actions/workflows/release.yml/badge.svg)](https://github.com/crushr3sist/blockframe-rs/actions/workflows/release.yml)

### What is BlockFrame?

Blockframe is a storage layer which automates every functionality which could be improved over conventional storage services. Blockframe turns a local machine into a durable, streamable data store akin to to something you'd rent from AWS or run through Databricks but without any account, subscription hell, IAM policies or network config.

### What problem's does this solve?

Contemporary data requirements have inflated to absurd limits due to availability of high-limit storage solutions, yet prices of storage and memory have sky-rocketed, leaving individual's who work with large files left buffering.

if you're a data scientist, machine learning engineer, or generally work with large files that need to stay in the cloud, your options are:

- pay for cloud storage, deal with egress fees, IAM, and ultimately deal with networking limitations.
- key raw huge files on a local drive, deal with drive failures, heat problems and electricity bills, and if a drive fails, that's it.
- set up your own NAS, which just basically means you value your data ownership, but you've not really saved any money, instead spent more in total than cloud storage.

None of these options provide a real "win" over cloud.

### What BlockFrame does

Blockframe ingests your files, ensures thier quality through erasure coding (Reed-Solomon ;)) to allow for mistakes with your important files such as whole deletes, then hands you a normal mounted filesystem. No API calls, no SDK, no special read paths, just point pandas or cat or literally anything else to those files and it will read them like a normal drive.

### BlockFrame Features

**Self-healing Storage**: Files are broken into shards using Reed-Solomon erasure coding, if you lose a chunk to disk corrpuition or user mistake, blockframe will reconstruct it automatically from parity, no need for backups or manual recovery.

**Tamper-evident by design**: Every shard is hashed and verified against a Merkle Tree before we let you touch it. If a byte flipped somewhere on disk, Blockframe catches it and repairs it before your code ever sees bad data.

**Mount it like a real drive**: BlockFrame presents your files to you through FUSE (linux) or WinFSP (windows), so your dataset shows up as a normal directory, no BlockFrame specific API required to read your own files, just click and read.

**Stream, not load:** A file is read segment-by-segment, not offsets, but segments, through the OS's normal file read path, backed by W-TinyLFU cache for hot segments. A 50GB dataset doesnt mean a 50GB memory spike.

**built for big files**: A tiered chunking strategy keeps segment sizes matched to how hard-drives actually perform, so your not paying a random-access penalty for durability.

**works over LAN too:** point BlockFrame at another machine running an instance on your network, and mountits archive the same way, no internet connection required, all local infrastructure.

### Why is BlockFrame important?

BlockFrame provides the same durability guarantee of a RAID array, or a cloud object store, without needding to dedicate a whole disk or system to a specific filesystem or hand your data to somone else's servers and deal with those limitations. Its local-first, your own, and behaves like a drive itself, and from the OS's perspective, it is one.

### status

BlockFrame is under halted but active development. Currently it can only provide read-only after ingest functionality, write-on-mount is currently on the back-burner, along with encryption and compression functionality. BlockFrame is open to contribution, however, the project is experimental and the code bases is not at the level of review, testing and QA required for stable production use.

### License

Copyright © 2026 Ronny (2Fresh)

BlockFrame is licensed under the Apache License, Version 2.0.

You may use, modify, and distribute this software under the terms of the license.

See the [LICENSE](LICENSE) file for the full license text.
