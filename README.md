# EIS GParted

Partition management tool for Linux and Android.

## Overview

EIS GParted is a partition editor for managing disk partitions. Provides safe partition creation, deletion, resizing, and formatting with verification.

## Stack

- Rust core for partition table operations
- Native Linux GUI (GTK4)
- Android companion (read-only inspection)

## Safety

- Requires explicit authorization for all destructive operations
- Verifies target is not system/internal storage
- Creates backup of partition table before modifications
- Fail-closed on ambiguous devices

## Build

```bash
cargo build
cargo run
```

## Roadmap

- [ ] M01: Project scaffold
- [ ] M02: Partition table reading (MBR/GPT)
- [ ] M03: Partition creation and deletion (gated)
- [ ] M04: Filesystem formatting (gated)
- [ ] M05: Partition resizing (gated)
- [ ] M06: Safety verification and confirmation
- [ ] M07: Android read-only inspection
