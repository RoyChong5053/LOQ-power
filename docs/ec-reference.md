# EC 与内核接口参考

本页记录 Lenovo LOQ 15ARP9（83JC，BIOS PQCN24WW，Ryzen 7 7435HS + RTX 4060
Laptop）上实际可用的接口，数值来自 `loq-power probe --json`。

## EC 固件属性

基址：`/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/`

| 属性 | 含义 | 单位 | 范围 | 默认 | 步进 |
|---|---|---|---|---|---|
| `gpu_nv_ctgp` | GPU 基础功耗 cTGP | W | 55..105 | 60 | — |
| `gpu_nv_ppab` | GPU 动态加速 PPAB | W | 10..25 | 15 | — |
| `gpu_nv_ac_offset` | GPU AC 额外偏移 | W | 10..55 | 45 | 1 |
| `gpu_nv_cpu_boost` | GPU 借给 CPU 的功耗 | W | 5..15 | 5 | — |
| `gpu_temp` | GPU 温度墙 | °C | 75..87 | 87 | 1 |
| `ppt_pl1_spl` | CPU 持续功耗 PL1/SPL | W | 40..75 | 54 | 1 |
| `ppt_pl2_sppt` | CPU 短时功耗 PL2/SPPT | W | 45..100 | 54 | 1 |
| `ppt_pl3_fppt` | CPU 峰值功耗 PL3/FPPT | W | 55..125 | 65 | 1 |
| `ppt_cpu_cl` | CPU 交叉负载预算 | W | 30..45 | 45 | 1 |
| `cpu_temp` | CPU 温度墙 | °C | 85..100 | 100 | 1 |

写入前提：热模式必须是 `custom`。`loq-core` 会自动先切到 `custom`（风扇基底取
`balanced`），写完由配置档恢复目标热模式。

> `gpu_nv_*` 是 EC 侧的整机预算，`nvidia-smi -pl` 在本机不可用（`power.limit`
> 为 N/A），因此 GPU 功耗只能通过这些属性调整。

## 热模式

`platform_profile` 同时由 ACPI 与 WMI gamezone 暴露：

```text
/sys/firmware/acpi/platform_profile
/sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile
```

可选值：`low-power`、`balanced`、`performance`、`max-power`、`custom`。

| 值 | 含义 | 备注 |
|---|---|---|
| `low-power` | 静音 | EC 写入不可用 |
| `balanced` | 均衡 | EC 写入不可用 |
| `performance` | 性能 | EC 写入不可用 |
| `max-power` | 极速 | 风扇更积极 |
| `custom` | 自定义 | 唯一允许写 EC 属性的模式 |

## 风扇

### EC 风扇模式

`/sys/bus/platform/drivers/ideapad_acpi/*/fan_mode`（可写）

| 值 | 行为 |
|---|---|
| 0 | 静音（Super Silent） |
| 1 | 标准 |
| 2 | 除尘（会明显变吵） |
| 4 | 强散热 / 全速（会明显变吵） |

> 读回值不一定与刚写入的值一致，驱动/固件可能报告当前生效档位而非请求值。
> 重启后不保留。

### hwmon

- `acpi_fan/fan1`：EC 风扇转速（RPM），只读
- `k10temp/Tctl`：CPU 温度

### LenovoLegionLinux（可选）

LOQ Power 不会加载树外模块；只有当 `legion_hwmon` 已经存在时才启用下列功能：

| 功能 | 接口 |
|---|---|
| 风扇/温度传感器 | `/sys/class/hwmon/*/fan*_input`、`temp*_input` |
| 风扇曲线 | `/sys/class/hwmon/*/pwm{1,2}_auto_point{N}_pwm`、`pwm{1,2,3}_auto_point{N}_temp` |
| 风扇全速 | `/sys/module/legion_laptop/drivers/*/*/fan_fullspeed` |
| LLL 电源模式 | `/sys/module/legion_laptop/drivers/*/*/powermode`（0 均衡 / 1 性能 / 2 安静） |
| 调试转储（只读） | `/sys/kernel/debug/legion/fancurve` |

风扇曲线走标准 hwmon 接口，不直接写 debugfs 或 EC 寄存器：

- `pwm1_auto_pointN_pwm` / `pwm2_auto_pointN_pwm`：第 N 点风扇 1/2 的 PWM（0–255）
- `pwm1_auto_pointN_temp`：第 N 点的 CPU 温度阈值
- `pwm2_auto_pointN_temp`：第 N 点的 GPU 温度阈值
- `pwm3_auto_pointN_temp`：第 N 点的 IC 温度阈值

写入前会校验点数与硬件一致、PWM ≤ 255、温度 ≤ 127，且每个序列随点号单调不减。
温度先写、转速后写，避免风扇在阈值就位前先加速。部分固件把 PWM 量化到 0–10 档，
回读时按最近档位接受（例如写入 70 读回 77）。

```bash
loq-power legion status
loq-power legion curve show            # TOML
loq-power legion curve save curve.toml
loq-power legion curve apply curve.toml
loq-power legion fullspeed true
loq-power legion powermode 1
```

GUI 的「风扇 / 电池」页在检测到 LLL 时显示曲线编辑表格、全速开关和电源模式。
切换 LLL 电源模式、重启或挂起后固件可能恢复默认曲线，需要重新写入。LLL 与内核
自带的 `lenovo_wmi_gamezone` 可能冲突，因此默认不安装、不加载。

## 电池

`/sys/class/power_supply/BAT1/`

| 属性 | 说明 |
|---|---|
| `charge_types` | `Fast`、`Standard`、`Long_Life` |
| `charge_type` | 当前档位；回读时以方括号标出当前项 |
| `capacity` | 电量百分比 |
| `status` | `Charging` / `Discharging` / `Not charging` / `Full` |
| `cycle_count` | 循环次数 |

`Long_Life` 对应约 80% 截止，无法设置任意阈值。

## CPU

`/sys/devices/system/cpu/cpu0/cpufreq/`

| 文件 | 说明 |
|---|---|
| `scaling_governor` | `performance` / `powersave` |
| `energy_performance_preference` | 可用值随电源状态变化（AC 上可能只有 `performance`） |
| `scaling_min_freq` / `scaling_max_freq` | 当前策略范围 |
| `cpuinfo_min_freq` / `cpuinfo_max_freq` | 硬件范围，约 411 MHz .. 4554 MHz |
| `/sys/devices/system/cpu/cpufreq/boost` | 0/1 |

写入时所有频率都会被夹到硬件范围内；`min_khz > max_khz` 会被拒绝。

## LED

`/sys/class/leds/`

| LED | 最大值 | 说明 |
|---|---|---|
| `platform::kbd_backlight` | 2 | 键盘背光：0 关 / 1 低 / 2 高 |
| `platform::fnlock` | 1 | Fn 锁指示灯，通常保持只读 |

## 省电开关（吸收自 TLP）

| 接口 | 路径 |
|---|---|
| PCIe ASPM | `/sys/module/pcie_aspm/parameters/policy` |
| NMI watchdog | `/proc/sys/kernel/nmi_watchdog` |
| WiFi 省电 | `iw dev <iface> set power_save on/off` |
| USB autosuspend | `/sys/bus/usb/devices/*/power/control`（跳过音频与打印机） |
| NVMe runtime PM | `/sys/block/nvme*n*/device/power/control` |
| HDA 省电 | `/sys/module/snd_hda_intel/parameters/power_save{,_controller}` |

## 无法通过用户空间写入的接口

- WMI `887B54E2-…` 与 capdata 设备的 `setable=0`，没有受支持的写入路径。
- `nvidia-smi` 的功耗上限在本机不可用；GPU 功耗只能通过 EC 属性调整。

如需访问原始 EC 寄存器，必须显式安装支持该功能的树外模块，并会触发
`org.loq.power.raw-ec` 授权；默认不提供该能力。
