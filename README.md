# LOQ Power

Lenovo LOQ 15ARP9（83JC）的 Linux 电源与 EC 控制工具。Rust 实现，提供命令行、
系统守护进程和 Tauri 桌面界面。

> 旧的 Python/GTK4 实现已归档到 [`legacy/`](legacy/)，仅作参考与回滚。
> 完整重写方案见 [`plan.md`](plan.md)。

## 功能

- **EC 功率控制**：CPU PL1/PL2/PL3/FPPT、交叉负载预算、GPU cTGP/PPAB/偏移、
  CPU/GPU 温度墙（写入自动进入 `custom` 模式）
- **热模式**：`low-power` / `balanced` / `performance` / `max-power` / `custom`
- **电池**：快速 / 标准 / 长寿三档充电
- **风扇**：EC 风扇模式（静音 / 标准 / 除尘 / 全速）、转速与温度传感器
- **CPU**：governor、EPP、频率上下限、boost
- **LED**：键盘背光
- **配置档**：内置省电/平衡/性能/自定义，可自定义 TOML
- **自动切换**：AC/电池变化、开机、挂起唤醒后重新应用配置档
- **吸收 TLP**：CPU 调频、PCIe ASPM、WiFi 省电、USB autosuspend、NVMe PM、
  HDA 省电、NMI watchdog；提供一键接管与回滚
- **桌面兼容**：实现 `net.hadess.PowerProfiles`，停用 `tlp-pd` 后 GNOME/KDE
  电源模式仍然可用
- **可选 LLL**：检测到 `legion_hwmon` 时显示风扇曲线编辑、全速开关与 powermode

## 目录

```
crates/loq-core    能力探测、sysfs 读写、配置档、TLP 导入、接管逻辑
crates/loq-cli     命令行 loq-power
crates/loq-helper  特权助手 loq-power-helper
crates/loq-powerd  系统守护进程 loq-powerd
crates/loq-gui     Tauri 2 外壳
ui/                Svelte 5 + Vite + TypeScript 前端
packaging/         systemd、D-Bus、polkit、desktop、安装脚本
docs/              架构、EC 参考、TLP 接管文档
legacy/            旧 Python 实现
```

## 构建

依赖：Rust 1.85+（edition 2024）、`bun`（或 npm）、GTK 3 与 WebKitGTK 4.1
（Tauri）、`nvidia-smi`（可选，读取 GPU 状态）、`iw`（WiFi 省电）。

```bash
# CLI / 守护进程 / 助手
cargo build --release -p loq-cli -p loq-helper -p loq-powerd

# 桌面界面（前端 + Tauri）
cd ui && bun install && bun run build && cd ..
cargo build --release -p loq-gui
```

开发 GUI 时先启动 Vite，再运行 Tauri：

```bash
cd ui && bun run dev          # http://localhost:1420
cargo run -p loq-gui          # 或 cargo tauri dev
```

## 安装

```bash
sudo ./packaging/install.sh
```

脚本安装二进制、systemd 单元、D-Bus 配置、polkit 策略、desktop 文件和示例配置，
但**不会**停用 TLP，也不会创建 `/etc/loq-power/profiles.toml`（否则接管向导无法
导入当前 TLP 配置）。需要自定义时复制示例文件即可。

手工安装时参考：

```bash
sudo install -m644 packaging/systemd/loq-powerd.service /usr/lib/systemd/system/
sudo install -m644 packaging/dbus/org.loq.Power1.conf /usr/share/dbus-1/system.d/
sudo install -m644 packaging/dbus/org.loq.Power1.service /usr/share/dbus-1/system-services/
sudo install -m644 packaging/polkit/org.loq.power.policy /usr/share/polkit-1/actions/
sudo install -m644 packaging/profiles.toml /etc/loq-power/profiles.toml.example
```

## 使用

```bash
loq-power status              # 状态摘要
loq-power probe --json        # 完整能力清单
loq-power platform get
loq-power ec list
loq-power --dry-run ec set gpu_nv_ppab 20
loq-power profile list
loq-power profile apply 性能  # 通过守护进程或 pkexec 应用
loq-power fan mode set 1
loq-power charge set Long_Life
loq-power cpu set --governor powersave --epp power
loq-power led set platform::kbd_backlight 1

# 仅在安装了 LenovoLegionLinux 时可用
loq-power legion status
loq-power legion curve show
loq-power legion curve apply curve.toml
```

`--json` 对所有子命令可用；`--dry-run` 只预览，不需要 root。

### 从 TLP 接管

```bash
loq-power --dry-run takeover   # 预览
sudo loq-power takeover        # 停用 tlp/tlp-pd，启动 loq-powerd
sudo loq-power restore-tlp     # 回滚
```

GUI 的「系统 → TLP 接管向导」提供同样的流程。细节见
[`docs/takeover.md`](docs/takeover.md)。

## 文档

- [架构与 D-Bus/polkit 接口](docs/architecture.md)
- [EC 与内核接口参考](docs/ec-reference.md)
- [TLP 接管与回滚](docs/takeover.md)
- [旧版开发笔记](legacy/DEVELOPMENT.md)
- [重写计划](plan.md)

## 开发

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && bun run check
```

单元测试使用临时目录模拟 sysfs（见 `crates/loq-core/tests/`），默认不会接触
真实硬件；真实写入只在显式调用 CLI/守护进程时发生。

## 许可

MIT
