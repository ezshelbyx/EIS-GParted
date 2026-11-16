//! EIS-GParted - Partition editor (Linux).
//!
//! Operates on disk image files (and block devices via sudo) using EIS-Storage-Core
//! for parsing and raw MBR/GPT table editing.

use eis_storage_core::partition::parse_partitions;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::process::{exit, Command};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("EIS-GParted v{VERSION} - Partition editor");
    println!();
    println!("Usage:");
    println!("  eis-gparted info <image>            Show partition table");
    println!("  eis-gparted list <image>");
    println!("  eis-gparted create-mbr <image>      Initialize an MBR signature");
    println!("  eis-gparted add-part <image> <start_lba> <size_lba> <type_hex> [bootable]");
    println!("  eis-gparted del-part <image> <index>");
    println!("  eis-gparted set-type <image> <index> <type_hex>");
    println!("  eis-gparted toggle-boot <image> <index>");
    println!("  eis-gparted format <image> <index> <fs>   (fs: vfat|ext4, requires sudo+losetup)");
    println!("  eis-gparted help");
}

fn read_image(path: &str) -> Vec<u8> {
    let mut f = fs::File::open(path).unwrap_or_else(|e| {
        eprintln!("open {path}: {e}");
        exit(1);
    });
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).unwrap();
    buf
}

fn write_image(path: &str, data: &[u8]) {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap_or_else(|e| {
            eprintln!("open {path}: {e}");
            exit(1);
        });
    f.write_all(data).unwrap();
    f.sync_all().unwrap();
}

fn cmd_info(path: &str) {
    let data = read_image(path);
    let (pt, parts) = parse_partitions(&data).unwrap_or_else(|e| {
        eprintln!("parse: {e}");
        exit(1);
    });
    println!("Image: {path} ({} bytes)", data.len());
    println!("Partition table: {:?}", pt);
    if parts.is_empty() {
        println!("  (no partitions)");
        return;
    }
    println!(
        "{:<4} {:<10} {:<10} {:<12} {:<6} {}",
        "IDX", "START", "END", "SIZE_BYTES", "TYPE", "BOOT"
    );
    for p in parts {
        println!(
            "{:<4} {:<10} {:<10} {:<12} 0x{:02x}     {}",
            p.index,
            p.start_lba,
            p.end_lba,
            p.size_bytes,
            p.partition_type,
            if p.bootable { "*" } else { "" }
        );
    }
}

fn cmd_create_mbr(path: &str) {
    let mut data = read_image(path);
    if data.len() < 512 {
        data.resize(512, 0);
    }
    data[510] = 0x55;
    data[511] = 0xAA;
    write_image(path, &data);
    println!("MBR signature written.");
}

fn mbr_slot_offset(idx: usize) -> usize {
    446 + idx * 16
}

fn cmd_add_part(path: &str, start: u64, size: u64, typ: u8, bootable: bool) {
    let mut data = read_image(path);
    // ensure mbr sig
    if data.len() < 512 {
        data.resize(512, 0);
    }
    data[510] = 0x55;
    data[511] = 0xAA;

    // find empty slot
    let mut slot: Option<usize> = None;
    for i in 0..4 {
        let off = mbr_slot_offset(i);
        if data[off + 4] == 0 && data[off..off + 4].iter().all(|&b| b == 0) {
            slot = Some(i);
            break;
        }
    }
    let idx = match slot {
        Some(i) => i,
        None => {
            eprintln!("No free MBR slot.");
            exit(1);
        }
    };
    let off = mbr_slot_offset(idx);
    // CHS set to 0 (LBA mode)
    data[off] = if bootable { 0x80 } else { 0x00 };
    data[off + 1] = 0;
    data[off + 2] = 0;
    data[off + 3] = 0;
    data[off + 4] = typ;
    data[off + 5] = 0;
    data[off + 6] = 0;
    data[off + 7] = 0;
    // start LBA (little-endian u32)
    let start32 = start as u32;
    let size32 = size as u32;
    for i in 0..4 {
        data[off + 8 + i] = ((start32 >> (8 * i)) & 0xff) as u8;
        data[off + 12 + i] = ((size32 >> (8 * i)) & 0xff) as u8;
    }
    write_image(path, &data);
    println!(
        "Added partition {} at LBA {} size {} sectors type 0x{:02x}.",
        idx, start, size, typ
    );
}

fn cmd_del_part(path: &str, idx: usize) {
    if idx > 3 {
        eprintln!("MBR partition index must be 0..3");
        exit(1);
    }
    let mut data = read_image(path);
    let off = mbr_slot_offset(idx);
    for i in 0..16 {
        data[off + i] = 0;
    }
    write_image(path, &data);
    println!("Deleted partition {}.", idx);
}

fn cmd_set_type(path: &str, idx: usize, typ: u8) {
    if idx > 3 {
        eprintln!("invalid index");
        exit(1);
    }
    let mut data = read_image(path);
    let off = mbr_slot_offset(idx);
    data[off + 4] = typ;
    write_image(path, &data);
    println!("Set partition {} type to 0x{:02x}.", idx, typ);
}

fn cmd_toggle_boot(path: &str, idx: usize) {
    if idx > 3 {
        eprintln!("invalid index");
        exit(1);
    }
    let mut data = read_image(path);
    let off = mbr_slot_offset(idx);
    data[off] ^= 0x80;
    write_image(path, &data);
    println!("Toggled boot flag on partition {}.", idx);
}

fn cmd_format(path: &str, idx: usize, fs: &str) {
    // use losetup to format the partition
    let loopdev = Command::new("sudo")
        .args(["losetup", "--find", "--show", "--partscan", path])
        .output()
        .expect("losetup");
    if !loopdev.status.success() {
        eprintln!(
            "losetup failed: {}",
            String::from_utf8_lossy(&loopdev.stderr)
        );
        exit(1);
    }
    let dev = String::from_utf8_lossy(&loopdev.stdout).trim().to_string();
    let part = format!("{dev}p{}", idx + 1);
    let ok = match fs {
        "vfat" | "fat32" => Command::new("sudo")
            .args(["mkfs.vfat", "-F", "32", &part])
            .status(),
        "ext4" => Command::new("sudo")
            .args(["mkfs.ext4", "-F", &part])
            .status(),
        "exfat" => Command::new("sudo").args(["mkfs.exfat", &part]).status(),
        _ => {
            eprintln!("unknown fs: {fs}");
            exit(1);
        }
    };
    let _ = Command::new("sudo").args(["losetup", "-d", &dev]).status();
    match ok {
        Ok(s) if s.success() => println!("Formatted partition {} as {}.", idx, fs),
        _ => {
            eprintln!("format failed");
            exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_help();
        return;
    }
    match args[1].as_str() {
        "info" | "list" => {
            if args.len() < 3 {
                eprintln!("missing image");
                exit(1);
            }
            cmd_info(&args[2]);
        }
        "create-mbr" => {
            if args.len() < 3 {
                eprintln!("missing image");
                exit(1);
            }
            cmd_create_mbr(&args[2]);
        }
        "add-part" => {
            if args.len() < 6 {
                eprintln!("usage: add-part <image> <start_lba> <size_lba> <type_hex> [bootable]");
                exit(1);
            }
            let start: u64 = args[3].parse().unwrap_or(0);
            let size: u64 = args[4].parse().unwrap_or(0);
            let typ = u8::from_str_radix(args[5].trim_start_matches("0x"), 16).unwrap_or(0);
            let bootable = args.get(6).map(|s| s == "bootable").unwrap_or(false);
            cmd_add_part(&args[2], start, size, typ, bootable);
        }
        "del-part" => {
            if args.len() < 4 {
                eprintln!("missing args");
                exit(1);
            }
            let idx: usize = args[3].parse().unwrap_or(0);
            cmd_del_part(&args[2], idx);
        }
        "set-type" => {
            if args.len() < 5 {
                eprintln!("missing args");
                exit(1);
            }
            let idx: usize = args[3].parse().unwrap_or(0);
            let typ = u8::from_str_radix(args[4].trim_start_matches("0x"), 16).unwrap_or(0);
            cmd_set_type(&args[2], idx, typ);
        }
        "toggle-boot" => {
            if args.len() < 4 {
                eprintln!("missing args");
                exit(1);
            }
            let idx: usize = args[3].parse().unwrap_or(0);
            cmd_toggle_boot(&args[2], idx);
        }
        "format" => {
            if args.len() < 5 {
                eprintln!("usage: format <image> <index> <fs>");
                exit(1);
            }
            let idx: usize = args[3].parse().unwrap_or(0);
            cmd_format(&args[2], idx, &args[4]);
        }
        "help" | "--help" | "-h" => print_help(),
        _ => {
            eprintln!("unknown command: {}", args[1]);
            print_help();
            exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_mbr_image(path: &str) {
        let mut data = vec![0u8; 1024 * 1024];
        data[510] = 0x55;
        data[511] = 0xAA;
        fs::write(path, &data).unwrap();
    }

    #[test]
    fn create_and_delete_partition() {
        let dir = std::env::temp_dir();
        let p = dir.join("eis_gparted_test.img");
        let ps = p.to_str().unwrap();
        make_mbr_image(ps);
        cmd_add_part(ps, 2048, 1024, 0x83, false);
        let data = read_image(ps);
        let (_, parts) = parse_partitions(&data).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start_lba, 2048);
        cmd_del_part(ps, 0);
        let data = read_image(ps);
        let (_, parts) = parse_partitions(&data).unwrap();
        assert!(parts.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn toggle_boot_flag() {
        let dir = std::env::temp_dir();
        let p = dir.join("eis_gparted_boot.img");
        let ps = p.to_str().unwrap();
        make_mbr_image(ps);
        cmd_add_part(ps, 2048, 1024, 0x83, false);
        cmd_toggle_boot(ps, 0);
        let data = read_image(ps);
        let (_, parts) = parse_partitions(&data).unwrap();
        assert!(parts[0].bootable);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn info_on_empty() {
        let dir = std::env::temp_dir();
        let p = dir.join("eis_gparted_empty.img");
        let ps = p.to_str().unwrap();
        make_mbr_image(ps);
        cmd_info(ps);
        let _ = fs::remove_file(&p);
    }
}
