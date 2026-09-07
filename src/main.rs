//! BORUIX `userdrv`：独立用户态驱动 ELF 模板（ADR-037 决策 2 / PRE-3）。
//!
//! 一个普通 no_std 用户 ELF，作为运行时驱动装载的二进制本体：**目标设备名
//! 经 argv[0] 传入**（exec_path 把整个 spawn cmd 字符串放进 argv[0]），驱动内部
//! register → claim → 映射 MMIO → volatile 读硬件寄存器 → unregister，证明一个
//! 用户态进程能作为该设备的驱动认领它并触达硬件窗口（与 shell 内建 uiodemo
//! 同一 UIO 机制，抽成可独立装载的 ELF）。
//!
//! 装载方式（由装载器/驱动 load 命令 spawn）：
//!   exec_path("/modules/<name>/driver.elf", b"<device-name>")
//! 本程序 user_main 读 argv[0] 得设备名（无参时回退默认 e1000）。
//!
//! 失败/权限：register/claim 是 System-only（ADR-037 PRE-1 / ADR-033）——本驱动
//! 若被 User 进程拉起会拿到 PermissionDenied 并如实报错退出（非 0），绝不自欺。

#![no_std]
#![no_main]
extern crate alloc;

use libsys::*;

/// 默认认领的 QEMU e1000 网卡（PCI BAR 有 MMIO 窗口，可被 UIO 认领）。
const DEFAULT_DEV: &[u8] = b"pci-ethernet-00-03-0";

/// 向 stdout 写一行（`[userdrv] ...`）。
fn say(msg: &[u8]) {
    let _ = write(STDOUT, b"[userdrv] ");
    let _ = write(STDOUT, msg);
    let _ = write(STDOUT, b"
");
}

fn u64_to_dec(mut v: u64, buf: &mut [u8; 24]) -> &[u8] {
    if v == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut i = buf.len();
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    &buf[i..]
}

/// 从 argv[0] 取出设备名字符串（argv[0] 是 exec_path 传入的整个 cmd 字节串）。
/// argc<1 或空 → 回退默认。
fn dev_name_from_argv(argc: isize, argv: *const *const u8) -> alloc::vec::Vec<u8> {
    if argc < 1 || argv.is_null() {
        return DEFAULT_DEV.to_vec();
    }
    let p = unsafe { *argv };
    if p.is_null() {
        return DEFAULT_DEV.to_vec();
    }
    let mut len = 0usize;
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    if len == 0 {
        return DEFAULT_DEV.to_vec();
    }
    unsafe { core::slice::from_raw_parts(p, len) }.to_vec()
}

/// 驱动全流程：query（读绑定态）→ register（认领）→ claim（映射 MMIO）→
/// volatile 读 reg[0] → unregister（释放）。任何一步失败如实报错返回非 0。
fn run_driver(dev: &[u8]) -> i32 {
    let name_str = core::str::from_utf8(dev).unwrap_or("?");
    let mut b = [0u8; 24];
    say(b"driver ELF started, target device =");
    let _ = write(STDOUT, dev);
    let _ = write(STDOUT, b"
");

    // 1. query：查设备绑定状态（只读，任何身份可查）。
    match driver_query(name_str) {
        Ok(json) => {
            say(b"query -> ");
            let _ = write(STDOUT, json.as_bytes());
            let _ = write(STDOUT, b"
");
        }
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_query failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"
");
            return 1;
        }
    }

    // 2. register：本进程认领该设备（System-only；User 会被 PermissionDenied 拒）。
    let uio_id = match driver_register(name_str) {
        Ok(id) => id,
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_register failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"
");
            return 1;
        }
    };
    say(b"registered (uio_id=");
    let _ = write(STDOUT, u64_to_dec(uio_id, &mut b));
    let _ = write(STDOUT, b")
");

    // 3. claim：授权映射 MMIO 窗口，返回用户虚拟地址。
    let va = match driver_claim(uio_id) {
        Ok(v) => v,
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_claim failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"
");
            let _ = driver_unregister(uio_id);
            return 1;
        }
    };
    say(b"claimed, device MMIO mapped at user 0x");
    let _ = write(STDOUT, u64_to_dec(va, &mut b));
    let _ = write(STDOUT, b" (dec)
");

    // 4. 从映射地址 volatile 读一个 32 位硬件寄存器（offset 0），证明用户态触达硬件。
    let reg: u32 = unsafe { core::ptr::read_volatile(va as *const u32) };
    say(b"read dev reg[0] = ");
    let _ = write(STDOUT, u64_to_dec(reg as u64, &mut b));
    let _ = write(STDOUT, b" (dec)
");

    // 5. unregister：释放认领（内核在进程退出时也会自动释放）。
    match driver_unregister(uio_id) {
        Ok(()) => say(b"unregistered ok
"),
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_unregister failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"
");
            return 1;
        }
    }
    say(b"PASS - userspace driver registered/claimed/mapped/read a real device");
    0
}

/// 用户程序入口（libsys `_start` 调用）。返回值为进程退出码。
#[unsafe(no_mangle)]
pub extern "C" fn user_main(argc: isize, argv: *const *const u8) -> i32 {
    let dev = dev_name_from_argv(argc, argv);
    run_driver(&dev)
}
