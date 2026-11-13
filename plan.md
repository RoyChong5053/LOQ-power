# LOQ Power — Rust 重写与 TLP 吸收计划

> 目标：把现有 Python/GTK4 项目重写为 Rust + Tauri，吸收 TLP 在本机真正生效的功能，
> 最终由 `loq-powerd` 作为唯一的电源管理守护进程，并尽可能覆盖这台
> Lenovo LOQ 15ARP9 (83JC) 能通过内核接口控制的 EC 功能。

- 机型：Lenovo LOQ 15ARP9 (83JC)，BIOS PQCN24WW
- CPU：AMD Ryzen 7 7435HS
- GPU：NVIDIA RTX 4060 Laptop
- 系统：CachyOS，kernel 7.2.8-1-cachyos
- 现有实现：`legacy/`（原 Python/GTK4 版本，仅作参考与回滚）

---

## 1. 目标与非目标

### 1.1 目标

1. Rust 重写现有全部功能：EC 功率、热模式、CPU 频率、仪表盘、方案管理。
2. GUI 使用 Tauri 2 + Svelte 5 + Vite + TypeScript + uPlot。
3. 吸收 TLP 在本机有效的功能，最终停用 `tlp.service` / `tlp-pd.service`，
   只保留 `loq-powerd`。
4. 覆盖内核已暴露的全部 EC/电源能力，并提供动态能力发现。
5. 提供安全、可回滚的特权操作：D-Bus + polkit + 严格白名单校验。
6. 支持 AC/BAT 自动切换、开机恢复、唤醒恢复。
7. 保持旧配置可迁移（`~/.config/loq-power/profiles.json` → TOML）。

### 1.2 非目标

- 不默认加载树外内核模块；LLL（LenovoLegionLinux）仅做“检测到才启用”。
- 不默认开放原始 EC 寄存器写入（`ec_sys`）。
- 不承诺 WMI A4 / capdata 的写入能力（当前 `setable=0`）。
- 不复制 TLP 的全部功能矩阵，只吸收本机实际生效、可验证的部分。
- 不在安装时自动停用 TLP；必须经过手动 takeover 向导。

---

## 2. 最终架构

```
┌─────────────────────────────────────┐
│ Tauri GUI（普通用户）                │
│ Svelte 5 + Vite + TS + uPlot        │
└───────────────┬─────────────────────┘
                │ D-Bus org.loq.Power1 / Tauri events
┌───────────────▼─────────────────────┐
│ loq-powerd（root，systemd 常驻）     │
│ • EC 功率 / 热模式 / 电池 / 风扇     │
│ • 吸收 TLP 的 CPU / 省电设置         │
│ • AC/BAT + 开机 + 唤醒自动应用       │
│ • net.hadess.PowerProfiles 兼容层    │
│ • polkit 鉴权 + 操作审计             │
└───────────────┬─────────────────────┘
                │ sysfs / nvidia-smi / iw
┌───────────────▼─────────────────────┐
│ 硬件：EC、gamezone、ideapad、BAT1    │
│ hwmon、cpufreq、PCIe、USB、GPU       │
└─────────────────────────────────────┘

fallback：没有 daemon 时，GUI 通过 pkexec 调 loq-power-helper
```

### 2.1 技术栈

| 层 | 选型 |
|---|---|
| GUI | Tauri 2 + Svelte 5 + Vite + TypeScript + uPlot |
| 核心逻辑 | `loq-core`（纯 Rust，无 GUI 依赖） |
| 系统守护 | `loq-powerd`（zbus + tokio + systemd） |
| 特权回退 | `loq-power-helper`（pkexec + JSON 协议） |
| CLI | `loq-power`（clap） |
| 配置 | TOML（`~/.config/loq-power/`） |
| 打包 | PKGBUILD、systemd、polkit、dbus policy、desktop file |

### 2.2 目录结构

```
loq-power/
├── Cargo.toml                 # workspace
├── plan.md
├── README.md
├── legacy/                    # 原 Python 实现
├── crates/
│   ├── loq-core/              # sysfs/EC/TLP 吸收逻辑
│   ├── loq-powerd/            # systemd 守护进程 + D-Bus + polkit
│   ├── loq-helper/            # pkexec 回退助手
│   ├── loq-cli/               # CLI + tlp 风格别名
│   └── loq-gui/               # Tauri 2 src-tauri
├── ui/                        # Svelte 5 + Vite + TS
├── packaging/
│   ├── PKGBUILD
│   ├── loq-powerd.service
│   ├── com.github.loq-power.policy
│   ├── org.loq.Power1.conf
│   └── com.github.loq-power.desktop
└── docs/
    ├── EC-MATRIX.md
    ├── TLP-ABSORPTION.md
    └── ARCHITECTURE.md
```

---

## 3. EC 能力矩阵

### 3.1 已确认可用的接口

| 能力 | 接口 | 范围/取值 | 写入要求 | 计划 |
|---|---|---|---|---|
| GPU cTGP | `gpu_nv_ctgp` | 55–105 W | CUSTOM | 滑块 + 范围校验 |
| GPU PPAB | `gpu_nv_ppab` | 10–25 W | CUSTOM | 滑块 |
| GPU AC Offset | `gpu_nv_ac_offset` | 10–55 W | CUSTOM | 滑块 |
| GPU→CPU Boost | `gpu_nv_cpu_boost` | 5–15 W | CUSTOM | 滑块 |
| GPU 温度墙 | `gpu_temp` | 75–87 °C | CUSTOM | 滑块 |
| CPU PL1 | `ppt_pl1_spl` | 40–75 W | CUSTOM | 滑块 |
| CPU PL2 | `ppt_pl2_sppt` | 45–100 W | CUSTOM | 滑块 |
| CPU PL3 | `ppt_pl3_fppt` | 55–125 W | CUSTOM | 滑块 |
| CPU 交叉负载 | `ppt_cpu_cl` | 30–45 W | CUSTOM | 滑块 |
| CPU 温度墙 | `cpu_temp` | 85–100 °C | CUSTOM | 滑块 |
| 热模式 | gamezone platform-profile | low-power / balanced / performance / max-power / custom | 否（平台写入） | 5 档 + custom 基底逻辑 |
| 电池充电 | `BAT1/charge_types` | Fast / Standard / Long_Life | 是 | 三档 |
| EC 风扇模式 | `ideapad_acpi/.../fan_mode` | 0 静音 / 1 标准 / 2 除尘 / 4 强散热 | 是 | 实验性 + 二次确认 |
| Fn Lock | `ideapad_acpi/.../fn_lock` | 0/1 | 是 | 开关 |
| 键盘背光 | `platform::kbd_backlight` | 0–2 | 是 | 亮度 |
| 平台 profile | `/sys/firmware/acpi/platform_profile` | 同 gamezone | 是 | 只读校验/回退 |
| 风扇转速 | `acpi_fan/fan1_input` | RPM | 否 | 监控 |
| CPU 温度 | `k10temp` Tctl/Tdie | °C | 否 | 监控 |
| GPU | `nvidia-smi` | 功耗/温度/利用率/显存/频率 | 否 | 监控 |

### 3.2 动态发现

`loq-core` 不硬编码属性名：

- 扫描 `/sys/class/firmware-attributes/lenovo-wmi-other-*/attributes/*`；
- 读取 `current_value`、`min_value`、`max_value`、`default_value`、`scalar_increment`；
- 根据名称推断单位（`*_temp` → °C，`gpu_*`/`ppt_*` → W）；
- 扫描 `ideapad_acpi/VPC2004:*/` 下所有可写属性，已知安全项开放写，未知项只读；
- 扫描 `/sys/class/leds/platform::*`；
- 扫描 `hwmon`、`thermal`、`power_supply`、`cpufreq`、`powercap`。

`loq-power probe --json` 输出完整能力清单，GUI 的 EC 浏览器直接消费。

### 3.3 暂不默认支持

| 能力 | 原因 |
|---|---|
| WMI A4 (`887B54E2`) | `setable=0`，无受支持写入口 |
| capdata | `setable=0`，只读元数据 |
| 原始 EC (`ec_sys`) | 无本机寄存器表，风险极高 |
| LLL 风扇曲线 | 树外模块，需用户显式安装；检测到才启用 |
| NVIDIA 功率上限写入 | 当前 `nvidia-smi` 查询为 N/A，先只读 |

### 3.4 可选：LenovoLegionLinux (LLL)

源码已确认支持 83JC / BIOS PQCN（`model_nzcn`）：

- `access_method_fancurve = ACCESS_METHOD_EC3`
- `access_method_fanspeed/temperature = ACCESS_METHOD_WMI3`
- 可读：风扇曲线、风扇 RPM、CPU/GPU/VRM 温度、powermode
- 可写：风扇曲线、风扇全速、powermode

集成原则：

- 检测 `/sys/class/hwmon/*/name == legion_hwmon`；
- 只在检测到时显示风扇曲线页；
- 不自动加载模块，不默认安装；
- 明确提示与内核 `lenovo-wmi-gamezone` 的潜在冲突；
- 提供“恢复固件默认曲线”。

---

## 4. TLP 吸收计划

### 4.1 本机 TLP 实际状态

- TLP 1.10.2，`tlp.service` + `tlp-pd.service` 均 active
- 当前 profile：`performance/AC`
- CPU：
  - AC：governor `performance`，EPP `performance`，1800000–4000000 kHz，boost on
  - BAT：governor `powersave`，EPP `low-power`，120000–2800000 kHz，boost on
- 平台：AC `balanced`，BAT `low-power`
- 电池：`Long_Life`（通过 `charge_types`）
- USB autosuspend：开（排除 audio/printer）
- PCIe ASPM：`default`
- WiFi 省电：AC off / BAT on
- NMI watchdog：0
- NVMe runtime PM：on

### 4.2 吸收清单

| TLP 功能 | 本机是否有效 | LOQ 处理 |
|---|---|---|
| CPU governor / EPP / min-max freq / boost | 是 | 原生实现，AC/BAT 两套 |
| Platform profile | 是 | 改用 gamezone，支持 5 档 |
| 电池阈值 / 充电模式 | 是 | 原生三档 |
| WiFi 省电 | 是 | `iw dev <iface> set power_save on/off` |
| USB autosuspend | 是 | 原生实现，保留排除规则 |
| PCIe ASPM | 是 | 写 `pcie_aspm.policy` |
| PCIe runtime PM | 是 | 原生实现，默认保守 |
| NVMe runtime PM / APM | 部分 | runtime PM 默认；APM 高级选项 |
| NMI watchdog | 是 | 原生实现 |
| 音频 power_save | 节点存在 | 可选，默认跟随 TLP |
| 磁盘 scheduler | 当前 kyber | 高级选项 |
| 蓝牙/WWAN/NFC/ThinkPad | 不适用 | 不做 |
| `net.hadess.PowerProfiles` | tlp-pd 提供 | 由 loq-powerd 兼容实现 |

### 4.3 接管流程

`loq-power takeover`：

1. 读取 `/etc/tlp.conf` 与 `/etc/tlp.d/*.conf`；
2. 导入 AC/BAT 的 CPU、平台、电池、省电设置；
3. 展示“将接管/将停用”的清单与 diff；
4. 用户显式确认；
5. `systemctl disable --now tlp tlp-pd`；
6. 启动 `loq-powerd`；
7. 验证 CPU / 平台 / 电池 / EC 写入；
8. 失败自动提示 `loq-power restore-tlp`。

`loq-power restore-tlp`：

1. 停止并禁用 `loq-powerd`；
2. `systemctl enable --now tlp tlp-pd`；
3. 恢复 TLP drop-in 备份；
4. 不删除 TLP 软件包和 `/etc/tlp.conf`。

安装时不自动接管；GUI 首次打开可提示，但必须手动确认。

### 4.4 兼容层

`loq-powerd` 在停用 `tlp-pd` 后提供：

- `org.loq.Power1`
- `net.hadess.PowerProfiles`（3 档：power-saver / balanced / performance）
- `org.freedesktop.UPower.PowerProfiles`（可选别名）

这样 GNOME/KDE 的电源模式组件仍然可用。

---

## 5. D-Bus 与权限

### 5.1 `org.loq.Power1`

方法：

- `GetState() -> a{sv}`
- `Probe() -> a{sv}`
- `ApplyProfile(s name, b dry_run) -> a{sv}`
- `SetEc(a{sv} values) -> a{sv}`
- `SetPlatform(s mode) -> a{sv}`
- `SetChargeType(s type) -> a{sv}`
- `SetFanMode(u mode) -> a{sv}`
- `SetFanCurve(a{sv} curve) -> a{sv}`
- `SetPowerSaving(s section, a{sv} values) -> a{sv}`
- `TakeoverTlp() -> a{sv}`
- `RestoreTlp() -> a{sv}`
- `GetHistory(u seconds) -> a{sv}`

属性：

- `ActiveProfile`、`PowerSource`、`ThermalMode`、`DaemonVersion`

信号：

- `StateChanged(a{sv})`、`ProfileChanged(s)`、`Error(s)`

### 5.2 polkit

| Action | 默认策略 |
|---|---|
| `org.loq.power.read` | 本地用户可读 |
| `org.loq.power.apply` | `auth_admin_keep` |
| `org.loq.power.takeover` | `auth_admin` |
| `org.loq.power.fan-curve` | `auth_admin` |
| `org.loq.power.raw-ec` | 默认不安装 |

daemon 通过 `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` 校验调用者。
可选安装参数允许 `allow_active=yes`（免密），默认不启用。

### 5.3 特权助手

`loq-power-helper` 仅用于 daemon 不可用时的回退：

- 通过 `pkexec` 调用；
- stdin 接收 JSON；
- 只接受白名单操作；
- 路径由助手解析，不信任客户端；
- 范围/步进校验；
- EC 先写、平台后写；
- 写完回读校验；
- 输出结构化 JSON 结果；
- 不调用 shell。

---

## 6. 配置与方案

### 6.1 配置目录

```
~/.config/loq-power/
├── config.toml
├── profiles.toml
└── history.db        # 可选，SQLite 或简单 JSONL
```

系统级：

```
/etc/loq-power/
├── config.toml       # daemon 配置
└── profiles.toml     # 可选的系统方案
```

### 6.2 方案模型

```toml
[profile.balanced]
platform = "balanced"
ec = { gpu_nv_ctgp = 60, gpu_nv_ppab = 15, ... }
cpu = { governor = "performance", epp = "performance", max_freq_mhz = 4000, min_freq_mhz = 1800, boost = true }
battery = { charge_type = "Standard" }
ideapad = { fan_mode = 1, fn_lock = true }
power_saving = { wifi = false, usb_autosuspend = true, pcie_aspm = "default" }

[power.ac]
profile = "performance"

[power.battery]
profile = "省电"
```

- 支持 AC/BAT 两套映射；
- 值在写入前按发现的范围 clamp，并给出警告；
- 内置方案与旧版默认值保持一致；
- `loq-power migrate` 把旧 `profiles.json` 迁移到 TOML。

---

## 7. GUI 设计

### 7.1 页面

1. **仪表盘**：GPU/CPU/系统卡片、实时功耗/温度/风扇曲线。
2. **功率**：EC 滑块（GPU/CPU）、方案栏、应用按钮。
3. **风扇**：`fan_mode` 四档；LLL 可用时显示风扇曲线编辑器。
4. **电池**：三档充电、健康度、循环次数、TLP 对照。
5. **系统省电**：CPU、WiFi、USB、PCIe、NVMe、NMI、音频。
6. **系统接管**：TLP 状态、导入 diff、接管/回滚。
7. **EC 浏览器**：动态属性表，标注来源、范围、可写性、风险。
8. **设置**：daemon 状态、polkit、自启动、语言、单位。

### 7.2 交互原则

- 仪表盘 2–3 秒刷新，后台线程读取，不阻塞 UI。
- 危险操作二次确认：`fan_mode` 2/4、风扇曲线、TLP 接管、原始 EC。
- 所有写操作支持 `--dry-run`（CLI）和“预览”按钮（GUI）。
- 状态中明确显示“会被 TLP 覆盖”等冲突警告。

---

## 8. CLI

```bash
loq-power status [--json]
loq-power probe [--json]
loq-power ec list|get|set
loq-power platform get|set
loq-power charge get|set
loq-power fan mode get|set
loq-power fan curve get|set
loq-power cpu get|set
loq-power power-saving get|set
loq-power tlp status|import|takeover|restore
loq-power profile list|show|apply|save|delete|export|import
loq-power daemon run|status
loq-power gui
loq-power migrate
```

TLP 风格别名：

```bash
loq-power ac | bat | performance | balanced | power-saver
loq-power setcharge <mode>
loq-power fullcharge
```

---

## 9. 分阶段实施

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| 0 | workspace + Tauri/Svelte 脚手架 + `probe` | `loq-power probe --json` 能列出本机能力 |
| 1 | `loq-core` 只读模型 + fake sysfs 测试 | 单测通过，覆盖 EC/平台/电池/CPU/GPU/TLP/LLL |
| 2 | `loq-core` 写操作 + helper | 能安全写 EC、平台、电池、fan_mode，并回读 |
| 3 | `loq-powerd` + D-Bus + polkit | `busctl` 可调用，权限正确，GUI 可连 |
| 4 | Tauri GUI 功能对等 | 覆盖旧版全部功能 |
| 5 | TLP 吸收 + takeover/rollback | 能导入 TLP 配置、停用 TLP、回滚 |
| 6 | AC/BAT + 开机/唤醒自动应用 | 插拔电、重启、唤醒后状态正确 |
| 7 | 可选 LLL 风扇曲线 | 检测到才显示，写入有校验和恢复 |
| 8 | 打包/文档/CI | PKGBUILD、desktop、polkit、README、CI 通过 |

---

## 10. 测试计划

- 单元测试：fake sysfs 树（临时目录），覆盖发现、范围校验、TLP 解析、方案合并。
- 集成测试：`--features hardware`，默认 `--dry-run`。
- 手工测试清单：
  - 每个 EC 属性写入 + 回读；
  - custom 模式切换与风扇基底；
  - 三档充电切换；
  - `fan_mode` 0/1/2/4 与恢复；
  - TLP 导入、接管、回滚；
  - AC/BAT 切换；
  - 重启、挂起/唤醒；
  - LLL 风扇曲线（如安装）。
- CI：`cargo fmt --check`、`cargo clippy`、`cargo test`、Svelte `npm run check`、Tauri build。

---

## 11. 风险与缓解

| 风险 | 缓解 |
|---|---|
| `custom` 模式被 TLP/固件重置 | daemon 在开机/AC-BAT/唤醒后重新断言 |
| TLP 接管后漏掉设置导致续航变差 | 导入 + 对照表 + 一键回滚 |
| `fan_mode` 读回不可靠/风扇狂转 | 二次确认、恢复标准、实验标记 |
| LLL 与内核 WMI 冲突 | 可选、检测到才启用、不自动加载 |
| USB autosuspend 影响输入设备 | 保留 TLP 排除规则，默认保守 |
| PCIe ASPM 影响稳定性 | 默认 `default`，高级选项需确认 |
| 原始 EC 写入风险 | 默认不开放，仅专家只读 |
| polkit/D-Bus 权限漏洞 | 白名单、路径由服务端解析、每方法鉴权 |
| Tauri WebKit 依赖 | 本机已满足，打包时声明依赖 |

---

## 12. 当前状态

- 旧 Python 实现已移至 `legacy/`，仅作参考与回滚。
- Phase 0–5 已完成：workspace、只读探测、写层与 helper、守护进程 + polkit、
  GUI 写入、TLP 导入/接管/回滚。
- Phase 6 已完成：AC/BAT 轮询切换、开机断言、logind 唤醒后重放。
- Phase 7 已完成：检测到 `legion_hwmon` 时提供风扇曲线、全速开关与 powermode；
  未安装时自动隐藏，绝不自动加载模块。
- Phase 8 进行中：安装脚本、PKGBUILD、desktop/polkit/D-Bus、文档与 CI 已就绪；
  剩余工作是真机验证与发布打包。
- 硬件验证：EC 写入/回读、热模式切换、PPD 兼容接口与 hold/release 已在本机
  （LOQ 15ARP9，PQCN24WW）验证；LLL 风扇曲线因未安装模块仅通过 fake sysfs
  测试。

---

*最后更新：2026-11-12*
