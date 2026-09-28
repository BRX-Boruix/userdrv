# userdrv

A **user-space driver template** for BORUIX: it demonstrates how a driver claims real hardware, maps its registers, and reads and writes them **from user space**.

[简体中文](README.md)

## The core idea: drivers need not live in the kernel

Putting drivers in the kernel carries a cost: **a bug in a piece of kernel code takes the whole system down**. And drivers tend to be the largest and most error-prone part of a kernel.

This template demonstrates the other path: **move the driver's implementation into user space and leave only "the authority to touch hardware" in the kernel**.

| Step | Where | What happens |
| --- | --- | --- |
| Register | Kernel | Declares "I want to drive this device" |
| Claim | Kernel | The kernel **grants** that device's hardware resources to this process |
| Map | Kernel | The device's register space is mapped into that process's address space |
| Read/write | **User space** | Driver code accesses hardware registers |

The crux is the last step: **the driver's main logic runs entirely in user space**. When it fails, only that process dies; the kernel and other programs are unaffected.

This repository is **not** a real driver but a **skeleton** demonstrating the complete call sequence: query the device, register, claim, map, read a register.

## Two modes

| Mode | Behaviour | Use |
| --- | --- | --- |
| **Transient** | Register, claim, read one register, release, exit | Manual verification, demonstration |
| **Resident** | Register, claim, **hold the claim**, read registers periodically | Automatic loading, serving the device continuously |

Resident mode waits by **sleeping rather than spinning**, yielding the CPU in between. A tight poll loop staring at the register would waste a whole CPU core for nothing.

## What happens when it is terminated

A resident driver may be force-terminated. Left unhandled, the device would be "claimed by a process that no longer exists" — **usable by nobody**.

The kernel's answer: on process exit it **automatically isolates that claim**, releasing the device. The managing program above can then restart the driver and the device becomes usable again — **a driver crash means "restart it", not "the device is gone forever"**.

## Privileges

Registering and claiming a device are **restricted operations**. Started under an ordinary user identity, the program receives "permission denied" and **exits with an honest error** — a program able to claim a hardware device can drive the hardware directly, unconstrained by ordinary file permissions, so the restriction is necessary.

## Hardware access requires volatile reads

Hardware registers are **not ordinary memory**. Compilers optimise ordinary memory access on the premise that "the value will not change by itself", but a register's value **is rewritten by the hardware**. Optimised as an ordinary variable (merging repeated reads into one, say), the driver would read a stale value.

Volatile access tells the compiler: **really go and read it every time**. This is fundamental to driver writing — code that is correct for ordinary memory is wrong here.

## Building

```bash
cargo build --release
```

## Layout

```
userdrv/
├── Cargo.toml    # package definition
├── build.rs      # injects the linker script
├── linker.ld     # user-space section layout
└── src/
    └── main.rs   # the driver skeleton in both modes
```

## Related projects

- [`libsys`](https://github.com/BRX-Boruix/libsys) — provides device registration, claim, and mapping interfaces
- [`driverd`](https://github.com/BRX-Boruix/driverd) — automatically loads user-space driver processes
- [`intel-hda`](https://github.com/BRX-Boruix/intel-hda) — a complete user-space driver in practice

## License

MIT License, copyright Yang Borui. See [LICENSE](LICENSE).
