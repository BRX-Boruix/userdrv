# userdrv

A BORUIX user-space driver template: how a driver claims real hardware, maps its registers and reads them, all from user space.

[简体中文](README.md)

## What it is

This is not a real driver but a skeleton: it demonstrates the full call sequence — query the device,
register, claim, map the registers, read the hardware. The kernel only keeps the granting of hardware
access; the driver logic runs entirely in user space, and a crash affects only this process.

## Two modes

- **Transient** — register, claim, read one register, release, exit. For manual verification and self-tests
- **Resident** — entered when the command line starts with `resident:<device>`: the claim is held, and the registers are read about every 2 seconds as a heartbeat proving the driver is servicing the device

## Permissions

Registering and claiming devices is restricted: running as an ordinary user yields "permission denied"
and an honest non-zero exit. A program allowed to claim hardware can act on it directly, outside
ordinary file permissions — the restriction is necessary.

## Known limitations

- The heartbeat reads real register values but drives no hardware behaviour; a real driver starts from this skeleton and adds its protocol
- Resident mode has no active exit path; when terminated, the kernel releases the device automatically

## Building

```bash
cargo build --release
```

## Repository layout

```
userdrv/
├── Cargo.toml    # package manifest
├── build.rs      # injects the linker script
├── linker.ld     # user-space segment layout
└── src/
    └── main.rs   # the two-mode driver skeleton
```

## Related projects

- [`libsys`](https://github.com/BRX-Boruix/libsys) — device register, claim and mapping interfaces
- [`driverd`](https://github.com/BRX-Boruix/driverd) — automatic loading of user-space drivers
- [`intel-hda`](https://github.com/BRX-Boruix/intel-hda) — a complete user-space driver instance

## License

MIT License, copyright Yang Borui. See [LICENSE](LICENSE).
