# userdrv

**简体中文** | [English](#english)

BORUIX 的**用户态驱动模板**——演示一个驱动程序如何在**用户态**认领真实硬件、映射它的寄存器
并读写。

```
[userdrv] registered (uio_id=1)
[userdrv] claimed, device MMIO mapped at user 0x7f000000 (dec)
[userdrv] read dev reg[0] = 0 (dec)
[userdrv] PASS (transient) - registered/claimed/mapped/read a real device
```

---

## 核心想法：驱动不必住在内核里

传统上驱动程序运行在内核态。这有它的道理——驱动要碰硬件寄存器，而硬件访问通常需要特权。

但把驱动放在内核里有个代价：**内核里的一段代码出错，整个系统就崩了**。驱动程序往往是内核
中最庞大、最容易出问题的部分（要处理各种硬件行为、时序、异常情况）。一个网卡驱动的空指针
就能让整台机器死掉。

这个模板演示的是另一条路：**把驱动的实现搬到用户态，只把"访问硬件的权限"这一件事留在内核里**。

具体做法是：

| 步骤 | 发生在哪 | 做什么 |
| --- | --- | --- |
| 登记 | 内核 | 用户态程序声明"我要驱动这个设备" |
| 认领 | 内核 | 内核把该设备的硬件资源**授权**给这个进程 |
| 映射 | 内核 | 设备的寄存器空间被映射进该进程的地址空间 |
| 读写 | **用户态** | 驱动代码像访问普通内存一样读写硬件寄存器 |

关键在于最后一步：**驱动的主体逻辑完全在用户态运行**。它出错只会让这个进程崩溃，内核和其他
程序不受影响。

## 这个模板是什么

它本身不是一个真正的硬件驱动——它是一个**骨架**，演示了完整的调用序列：查询设备、登记、认领、
映射、读寄存器。

真实的用户态驱动应该以它为模板，把"读一次寄存器"替换成真正的设备逻辑。

## 两种运行模式

程序根据启动参数选择模式：

| 模式 | 行为 | 用途 |
| --- | --- | --- |
| **瞬态** | 登记 → 认领 → 读一次寄存器 → 释放 → 退出 | 手动验证、演示 |
| **长驻** | 登记 → 认领 → **保持认领**，周期性读寄存器 | 自动装载、持续服务设备 |

**瞬态模式**是"看一眼就走"：确认整条链路能跑通，读完立刻释放资源。适合手动测试和演示。

**长驻模式**才是驱动真正工作的形态：它保持对设备的认领，周期性地与设备交互证明自己仍在服务，
直到被终止。

长驻模式里一个值得注意的细节：**它以睡眠而非忙等的方式等待**。驱动每隔一段时间读一次寄存器，
中间的时间用来睡眠、让出 CPU。如果改成紧紧盯着寄存器的忙等循环，就会白白占满一个 CPU 核心
——对驱动来说完全没有必要。

## 被终止时会发生什么

长驻驱动可能被强制终止。这时有一个关键问题：**它对设备的认领怎么办？**

如果内核不处理，那个设备就会处于"被一个已经不存在的进程认领着"的状态——**谁也用不了它**。

内核的处理是：在进程退出时**自动隔离它的认领**，把设备释放出来。于是它上层的管理程序可以
重新拉起这个驱动，设备重新可用。

这条路径让驱动的**崩溃不再意味着设备永久失效**——重启驱动即可恢复。

## 权限

登记与认领设备是**受限操作**：如果这个程序被一个普通用户身份的进程拉起，它会收到"权限不足"
并**如实报错退出**。

这是必要的——能认领硬件设备的程序实际上可以直接操作硬件，不受普通文件权限的约束。

程序在这里的纪律是：**权限不足就如实失败，绝不假装成功**。

## 硬件读写

映射完成后，驱动通过**易失性读取**访问硬件寄存器。这里必须用易失性访问，原因是：

硬件寄存器**不是普通内存**。编译器优化普通内存访问的前提是"值不会自己变"，但寄存器的值会
**被硬件改写**。如果编译器把它当普通变量做优化（比如把重复读取合并成一次），驱动程序就会
读到过期的值。易失性访问告诉编译器：**每次都要真的去读**。

这是写驱动的基本功，也是最容易犯的错误之一——在普通内存上正确的代码，用在这里就是错的。

## 输出

程序逐步骤打印过程：登记结果、认领状态、映射到的地址、读到的寄存器值。

瞬态模式成功时打印 `PASS`，失败时打印具体是哪一步出了问题并返回非零退出码。

## 构建

```bash
cargo build --release
```

编译产物是一个普通的用户态程序，由系统的驱动管理程序或手动命令拉起。

## 文件结构

```
userdrv/
├── Cargo.toml    # 包定义
├── build.rs      # 注入链接脚本
├── linker.ld     # 用户态段布局
└── src/
    └── main.rs   # 两种模式的驱动骨架
```

## 相关项目

- [`libsys`](https://github.com/BRX-Boruix/libsys) —— 提供设备登记、认领与映射接口
- [`driverd`](https://github.com/BRX-Boruix/driverd) —— 自动装载用户态驱动进程
- [`intel-hda`](https://github.com/BRX-Boruix/intel-hda) —— 一个完整的用户态驱动实例

## 许可

MIT License，版权归 Yang Borui 所有。详见 [LICENSE](LICENSE)。

---

# English

[简体中文](#userdrv) | **English**

A **user-space driver template** for BORUIX — it demonstrates how a driver can claim real hardware,
map its registers, and read and write them **from user space**.

```
[userdrv] registered (uio_id=1)
[userdrv] claimed, device MMIO mapped at user 0x7f000000 (dec)
[userdrv] read dev reg[0] = 0 (dec)
[userdrv] PASS (transient) - registered/claimed/mapped/read a real device
```

---

## The core idea: drivers need not live in the kernel

Drivers traditionally run in kernel mode. There is a reason — a driver touches hardware registers,
and hardware access normally requires privilege.

But putting drivers in the kernel carries a cost: **a bug in a piece of kernel code takes the whole
system down**. Drivers tend to be the largest and most error-prone part of a kernel (contending with
all manner of hardware behaviour, timing, and exceptional conditions). One null dereference in a NIC
driver can kill the machine.

This template demonstrates the other path: **move the driver's implementation into user space and leave
only "the authority to touch hardware" in the kernel**.

Concretely:

| Step | Where | What happens |
| --- | --- | --- |
| Register | Kernel | The user-space program declares "I want to drive this device" |
| Claim | Kernel | The kernel **grants** that device's hardware resources to this process |
| Map | Kernel | The device's register space is mapped into that process's address space |
| Read/write | **User space** | Driver code accesses hardware registers like ordinary memory |

The crux is the last step: **the driver's main logic runs entirely in user space**. When it fails,
only that process dies; the kernel and other programs are unaffected.

## What this template is

It is not a real hardware driver — it is a **skeleton** demonstrating the complete call sequence:
query the device, register, claim, map, read a register.

A real user-space driver should use it as a template, replacing "read one register" with genuine
device logic.

## Two modes

The program selects a mode from its startup arguments:

| Mode | Behaviour | Use |
| --- | --- | --- |
| **Transient** | Register, claim, read one register, release, exit | Manual verification, demonstration |
| **Resident** | Register, claim, **hold the claim**, read registers periodically | Automatic loading, serving the device continuously |

**Transient mode** is look-and-leave: confirm the whole chain works, then release the resources
immediately. Good for manual testing and demos.

**Resident mode** is the shape a driver actually works in: it holds the claim on the device and
periodically interacts with it to show it is still serving, until terminated.

One detail worth noting in resident mode: **it waits by sleeping, not by spinning**. The driver reads a
register at intervals and sleeps in between, yielding the CPU. A tight poll loop staring at the
register would waste a whole CPU core for nothing — entirely unnecessary for a driver.

## What happens when it is terminated

A resident driver may be force-terminated. That raises a crucial question: **what becomes of its claim
on the device?**

If the kernel did nothing, the device would be left "claimed by a process that no longer exists" —
**usable by nobody**.

The kernel's answer: on process exit it **automatically isolates that claim**, releasing the device. The
managing program above can then restart the driver and the device becomes usable again.

That path is what makes a driver **crash mean "restart it", not "the device is gone forever"**.

## Privileges

Registering and claiming a device are **restricted operations**: started under an ordinary user
identity, this program receives "permission denied" and **exits with an honest error**.

That is necessary — a program able to claim a hardware device can drive the hardware directly,
unconstrained by ordinary file permissions.

The discipline here is: **when privilege is insufficient, fail honestly and never pretend success**.

## Hardware access

Once mapped, the driver reaches hardware registers through **volatile reads**. The volatile access is
mandatory because:

Hardware registers are **not ordinary memory**. Compilers optimise ordinary memory access on the
premise that "the value will not change by itself", but a register's value **is rewritten by the
hardware**. Optimised as an ordinary variable (merging repeated reads into one, say), the driver would
read a stale value. Volatile access tells the compiler: **really go and read it every time**.

This is fundamental to driver writing and among the easiest mistakes to make — code that is correct
for ordinary memory is wrong here.

## Output

The program prints each step: the registration result, the claim state, the mapped address, and the
register value it read.

In transient mode a success prints `PASS`; a failure prints which step failed and returns a non-zero
exit code.

## Building

```bash
cargo build --release
```

The artifact is an ordinary user-space program, started by the system's driver manager or by a manual
command.

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
