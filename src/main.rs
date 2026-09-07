//! BORUIX `userdrv`：独立用户态驱动 ELF 模板（ADR-037 决策 2 / PRE-3）。
//!
//! 一个普通 no_std 用户 ELF，作为运行时驱动装载的二进制本体：**目标设备名
//! 经 argv[0] 传入**（exec_path 把整个 spawn cmd 字符串放进 argv[0]），驱动内部
//! register → claim → 映射 MMIO → volatile 读硬件寄存器。
//!
//! 双模式（argv 前缀区分）：
//!   - 瞬态模式：argv[0] = "<device-name>"（Phase-1 / 手动 `driver load`），register →
//!     claim → 读一次 MMIO → unregister → 退出（演示/自检，读后即释放）。
//!   - 长驻模式：argv[0] = "resident:<device-name>"（driverd 自动装载用，P2-1/P2-2），
//!     register → claim → **保持认领不释放**，周期 heartbeat 读 MMIO（证明驱动在服务），
//!     直到被 SIGKILL/退出——内核 `uio_on_process_exit` 自动隔离其认领，driverd 收尸后
//!     可重拉（P2-2）。
//!
//! 失败/权限：register/claim 是 System-only（ADR-037 PRE-1 / ADR-033）——本驱动若被
//! User 进程拉起会拿到 PermissionDenied 并如实报错退出（非 0），绝不自欺。

#![no_std]
#![no_main]
extern crate alloc;

use libsys::*;

/// 默认认领的 QEMU e1000 网卡（PCI BAR 有 MMIO 窗口，可被 UIO 认领）。
const DEFAULT_DEV: &[u8] = b"pci-ethernet-00-03-0";
/// 长驻模式 argv 前缀。
const RESIDENT_PREFIX: &[u8] = b"resident:";

/// 向 stdout 写一行（`[userdrv] ...`）。
fn say(msg: &[u8]) {
    let _ = write(STDOUT, b"[userdrv] ");
    let _ = write(STDOUT, msg);
    let _ = write(STDOUT, b"\n");
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

/// 从 argv[0] 解析：返回 (长驻? , 设备名)。argv[0] 是 exec_path 传入的整个 cmd。
/// 空/无参 → 瞬态 + 默认 e1000。
fn parse_argv(argc: isize, argv: *const *const u8) -> (bool, alloc::vec::Vec<u8>) {
    if argc < 1 || argv.is_null() {
        return (false, DEFAULT_DEV.to_vec());
    }
    let p = unsafe { *argv };
    if p.is_null() {
        return (false, DEFAULT_DEV.to_vec());
    }
    let mut len = 0usize;
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    let raw = unsafe { core::slice::from_raw_parts(p, len) };
    if raw.starts_with(RESIDENT_PREFIX) {
        let dev = &raw[RESIDENT_PREFIX.len()..];
        if dev.is_empty() {
            return (true, DEFAULT_DEV.to_vec());
        }
        return (true, dev.to_vec());
    }
    if raw.is_empty() {
        return (false, DEFAULT_DEV.to_vec());
    }
    (false, raw.to_vec())
}

/// 认领设备并映射 MMIO；成功返回 (uio_id, 用户 VA)，失败如实报错返回 None。
fn claim_device(name_str: &str, uio_ret: &mut u64, va_ret: &mut u64) -> bool {
    let mut b = [0u8; 24];
    match driver_register(name_str) {
        Ok(id) => *uio_ret = id,
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_register failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"\n");
            return false;
        }
    }
    say(b"registered (uio_id=");
    let _ = write(STDOUT, u64_to_dec(*uio_ret, &mut b));
    let _ = write(STDOUT, b")\n");
    match driver_claim(*uio_ret) {
        Ok(v) => *va_ret = v,
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_claim failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"\n");
            let _ = driver_unregister(*uio_ret);
            return false;
        }
    }
    say(b"claimed, device MMIO mapped at user 0x");
    let _ = write(STDOUT, u64_to_dec(*va_ret, &mut b));
    let _ = write(STDOUT, b" (dec)\n");
    true
}

/// 驱动主逻辑（两模式共用认领部分，差异在退出路径）。
fn run(dev: &[u8], resident: bool) -> i32 {
    let name_str = core::str::from_utf8(dev).unwrap_or("?");
    let mut b = [0u8; 24];
    say(b"driver ELF started, target device =");
    let _ = write(STDOUT, dev);
    let _ = write(STDOUT, b"\n");

    // 1. query：查设备绑定/UIO 态（只读，任何身份可查）。
    match driver_query(name_str) {
        Ok(json) => {
            say(b"query -> ");
            let _ = write(STDOUT, json.as_bytes());
            let _ = write(STDOUT, b"\n");
        }
        Err(e) => {
            let _ = write(STDOUT, b"[userdrv] driver_query failed: ");
            let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
            let _ = write(STDOUT, b"\n");
            return 1;
        }
    }

    // 2-3. register + claim + map MMIO。
    let mut uio_id: u64 = 0;
    let mut va: u64 = 0;
    if !claim_device(name_str, &mut uio_id, &mut va) {
        return 1;
    }

    if !resident {
        // ---- 瞬态模式：读一次即释放退出（Phase-1 手动 driver load 演示）。
        let reg: u32 = unsafe { core::ptr::read_volatile(va as *const u32) };
        say(b"read dev reg[0] = ");
        let _ = write(STDOUT, u64_to_dec(reg as u64, &mut b));
        let _ = write(STDOUT, b" (dec)\n");
        match driver_unregister(uio_id) {
            Ok(()) => say(b"unregistered ok\n"),
            Err(e) => {
                let _ = write(STDOUT, b"[userdrv] driver_unregister failed: ");
                let _ = write(STDOUT, alloc::format!("{:?}", e).as_bytes());
                let _ = write(STDOUT, b"\n");
                return 1;
            }
        }
        say(b"PASS (transient) - registered/claimed/mapped/read a real device");
        0
    } else {
        // ---- 长驻模式：保持认领，周期 heartbeat 读 MMIO（证明驱动在服务设备），
        //      直到被 SIGKILL/退出——内核自动隔离其认领，driverd 收尸重拉(P2-2)。
        say(b"RESIDENT mode: holding claim, servicing device (heartbeat ~2s)");
        let mut n: u32 = 0;
        loop {
            let reg: u32 = unsafe { core::ptr::read_volatile(va as *const u32) };
            let _ = write(STDOUT, b"[userdrv] heartbeat #");
            let _ = write(STDOUT, u64_to_dec(n as u64, &mut b));
            let _ = write(STDOUT, b" reg[0]=");
            let _ = write(STDOUT, u64_to_dec(reg as u64, &mut b));
            let _ = write(STDOUT, b"\n");
            n += 1;
            // 睡 2s：不忙转、不抢 CPU（与 volumed 对账同阶）。期间被 SIGKILL 即隔离。
            let _ = sleep(2_000_000_000);
        }
    }
}

/// 用户程序入口（libsys `_start` 调用）。返回值为进程退出码。
#[unsafe(no_mangle)]
pub extern "C" fn user_main(argc: isize, argv: *const *const u8) -> i32 {
    let (resident, dev) = parse_argv(argc, argv);
    run(&dev, resident)
}