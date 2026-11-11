# LOQ Power Control - 开发知识文档

本文档记录项目开发过程中的关键技术发现、架构决策和调试经验。

## 1. 硬件信息

- **机型**: Lenovo LOQ 15ARP9 (83JC)
- **BIOS**: PQCN24WW
- **CPU**: AMD Ryzen 7 7435HS (8 核 16 线程, 最高 4553 MHz)
- **GPU**: NVIDIA GeForce RTX 4060 Laptop (8GB)
- **系统**: CachyOS, kernel 7.1.4-1-cachyos
- **双系统**: SSD1 = Win11, SSD2 = CachyOS

## 2. EC 固件接口

### 2.1 sysfs 路径

EC 属性通过 `lenovo_wmi_other` 内核模块暴露:

```
/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/
├── gpu_nv_ctgp/
│   ├── current_value    # 当前值 (可读写)
│   ├── min_value        # 最小值
│   ├── max_value        # 最大值
│   └── default_value    # 默认值
├── gpu_nv_ppab/
├── gpu_nv_ac_offset/
├── gpu_nv_cpu_boost/
├── gpu_temp/
├── ppt_pl1_spl/
├── ppt_pl2_sppt/
├── ppt_pl3_fppt/
├── ppt_cpu_cl/
└── cpu_temp/
```

### 2.2 读写权限

- **读取**: 无需 root
- **写入**: 需要 root (通过 `sudo tee`)
- **写入限制**: 内核模块检查 `mode == LWMI_GZ_THERMAL_MODE_CUSTOM (0xFF)`，否则返回 `-EBUSY`

### 2.3 EC 属性详情

| 属性名 | 名称 | 范围 | 单位 | 说明 |
|--------|------|------|------|------|
| `gpu_nv_ctgp` | GPU cTGP | 55-105 | W | 可配置总图形功率 |
| `gpu_nv_ppab` | GPU PPAB | 10-25 | W | 功率加速 |
| `gpu_nv_ac_offset` | GPU AC Offset | 10-55 | W | 电源适配器功率偏移 |
| `gpu_nv_cpu_boost` | GPU→CPU Boost | 5-15 | W | 动态加速 |
| `gpu_temp` | GPU 温度限制 | 75-87 | °C | GPU 降频温度墙 |
| `ppt_pl1_spl` | CPU PL1 | 40-75 | W | 持续功率 |
| `ppt_pl2_sppt` | CPU PL2 | 45-100 | W | 短时功率 |
| `ppt_pl3_fppt` | CPU PL3 | 55-125 | W | 快速功率跟踪 |
| `ppt_cpu_cl` | CPU 交叉负载 | 30-45 | W | GPU 活动时 CPU 功率 |
| `cpu_temp` | CPU 温度限制 | 85-100 | °C | CPU 降频温度墙 |

## 3. CUSTOM 热模式

### 3.1 问题

`lenovo_wmi_other` 驱动的 `attr_current_value_store()` 函数:

```c
ret = lwmi_tm_notifier_call(&mode);
if (ret)
    return ret;
if (mode != LWMI_GZ_THERMAL_MODE_CUSTOM)
    return -EBUSY;
```

写入 EC 属性前检查热模式，只有 CUSTOM (0xFF) 才允许写入。

### 3.2 解决方案

Fn+Q 在 LOQ 上的循环: quiet → balanced → performance → max-power → quiet
**不包含 CUSTOM 模式** (与 Legion 不同)。

但 gamezone WMI 驱动有一个 platform-profile 接口:

```
/sys/bus/wmi/drivers/lenovo_wmi_gamezone/887B54E3-DDDC-4B2C-8B88-68A26A8835D0-21/
└── platform-profile/
    └── platform-profile-0/
        ├── profile        # 读写: 当前模式
        └── choices        # 只读: low-power balanced performance max-power custom
```

写入 `custom` 即可切换到 CUSTOM 模式:

```bash
echo "custom" | sudo tee /sys/bus/wmi/drivers/lenovo_wmi_gamezone/.../platform-profile-0/profile
```

### 3.3 关键发现: Fn+Q vs 软件写入

**这是项目最重要的发现之一:**

| | Fn+Q (硬件中断) | 软件写 platform-profile |
|---|---|---|
| **触发方式** | 按键 → EC 中断 | WMI 调用 → EC |
| **LED 颜色** | ✅ 会变 | ❌ 不变 |
| **风扇策略** | ✅ 会变 | ❌ 不变 |
| **thermal_mode** | ✅ 会设 | ✅ 会设 |
| **EC 值可写** | ❌ 仅 custom 时 | ✅ 设了 custom 就行 |

**Fn+Q 的路径:**
```
Fn+Q → EC 中断 → gamezone driver → thermal_mode_notify()
→ 同时做三件事:
  a) 切换 EC 内部热策略 (风扇曲线/功率分配)
  b) 改变 LED 颜色
  c) 设置 thermal_mode 寄存器
```

**软件写入的路径:**
```
echo custom > profile → gamezone platform_profile_set()
→ WMI 调用 → EC
→ EC 只做一件事: 设置 thermal_mode 寄存器
→ 不触发 LED 变化 (因为不是 Fn+Q 中断)
→ 不触发风扇策略变化 (因为 EC 内部策略由 Fn+Q 决定)
```

### 3.4 LED 颜色映射

| Fn+Q 模式 | LED 颜色 | 风扇行为 | EC 热策略 |
|-----------|----------|----------|-----------|
| low-power | 🔵 蓝灯 | 安静 | 激进省电 |
| balanced | ⚪ 白灯 | 正常 | 均衡 |
| performance | 🔴 红灯 | 积极 | 性能优先 |
| max-power | 🟣 紫灯 | 全速 | 最大性能 |
| custom | 🟣 紫灯 | 取决于 Fn+Q | 取决于 Fn+Q |

**关键发现:**
- Custom 模式在软件切换后，LED 颜色保持上次 Fn+Q 的颜色
- 风扇行为也保持上次 Fn+Q 的策略
- EC 值变得可写 (因为 mode==CUSTOM)

### 3.5 应用架构

基于上述发现，我们的应用架构:

1. 用户用 Fn+Q 选择想要的风扇/LED 模式
2. 我们的工具应用时: 切 custom → 写 EC 值 → (可选)切回原模式
3. EC 值写入后持久保存，即使之后切回 balanced/performance 也保持

**简化后的理解:**
- Custom 模式 = 骗过内核检查的"后门"
- Fn+Q = 用户控制风扇/LED 的界面
- EC 值 = 持久保存的功率参数

### 3.6 风扇行为

- **max-power 模式**: EC 等待 Lenovo 软件发送风扇曲线 → 未收到 → 安全模式 → 风扇全速
- **custom 模式**: EC 行为不同，不强制风扇全速，正常温控
- **其他模式**: EC 使用内部预设的风扇策略

## 4. WMI 接口

### 4.1 已知 WMI GUID

| GUID | object_id | 驱动 | 说明 |
|------|-----------|------|------|
| `887B54E2-DDDC-4B2C-8B88-68A26A8835D0` | A4 | 无 | 可能是 SetSmartFanMode |
| `887B54E3-DDDC-4B2C-8B88-68A26A8835D0` | AA | lenovo_wmi_gamezone | platform-profile |
| `DC2A8805-3A8C-41BA-A6F7-092E0089CD3B` | AE | lenovo_wmi_other | EC 属性 |
| `D320289E-8FEA-41E0-86F9-911D83151B5F` | - | lenovo_wmi_events | 事件 |
| `CE6C0974-0407-4F50-88BA-4FC3B6559AD8` | SK | lenovo_wmi_hotkey_utilities | 热键 |

### 4.2 内核模块依赖

```
lenovo_wmi_other
├── depends: lenovo-wmi-helpers, wmi, lenovo-wmi-events
└── import_ns: LENOVO_WMI_HELPERS

lenovo_wmi_gamezone
├── depends: lenovo-wmi-helpers, wmi, lenovo-wmi-events
└── import_ns: LENOVO_WMI_HELPERS, LENOVO_WMI_EVENTS
```

### 4.3 thermal_mode 枚举 (wmi-helpers.h)

```c
enum lwmi_thermal_mode {
    LWMI_GZ_THERMAL_MODE_NONE      = 0x00,
    LWMI_GZ_THERMAL_MODE_QUIET     = 0x01,
    LWMI_GZ_THERMAL_MODE_BALANCED  = 0x02,
    LWMI_GZ_THERMAL_MODE_PERFORMANCE = 0x03,
    LWMI_GZ_THERMAL_MODE_EXTREME   = 0xE0,
    LWMI_GZ_THERMAL_MODE_CUSTOM    = 0xFF,
};
```

## 5. 辅助脚本

`loq-power-apply` 通过 `sudo` 执行，从 stdin 读取 `key=value` 对:

```bash
#!/bin/bash
# 输入格式:
# ec_gpu_nv_ctgp=60
# ec_ppt_pl1_spl=45
# cpu_max_freq=4000000
# cpu_governor=performance
# cpu_epp=performance
# platform_profile=custom
```

脚本自动查找:
- EC 属性路径: `/sys/class/firmware-attributes/lenovo-wmi-other-*/attributes/`
- gamezone profile: `/sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile`

## 6. 其他监控数据

### 6.1 GPU (nvidia-smi)

```bash
nvidia-smi --query-gpu=name,temperature.gpu,utilization.gpu,utilization.memory,memory.used,memory.total,power.draw --format=csv,noheader,nounits
```

### 6.2 CPU 温度

```
/sys/class/thermal/thermal_zone0/temp  # 值需除以 1000
```

### 6.3 电池充电模式

Lenovo LOQ 支持三种电池充电模式，通过 `charge_types` 控制:

```
/sys/class/power_supply/BAT1/charge_types
# 输出: "Fast Standard [Long_Life]"
# 方括号 [] 表示当前激活的模式
```

| 模式 | 说明 | 充电上限 |
|------|------|----------|
| Fast | 快速充电 | 100% |
| Standard | 标准充电 | 100% |
| Long_Life | 长寿模式 | 80% |

读取当前模式:
```python
def read_battery_charge_type():
    ct = read_file("/sys/class/power_supply/BAT1/charge_types")
    # 解析 "Fast Standard [Long_Life]" 格式
    current = None
    available = []
    for token in ct.split():
        if token.startswith("[") and token.endswith("]"):
            current = token.strip("[]")
        available.append(token.strip("[]"))
    return current, available
```

切换模式:
```bash
echo "Fast" | sudo tee /sys/class/power_supply/BAT1/charge_types
```

### 6.3 电池

```
/sys/class/power_supply/BAT1/
├── capacity       # 电量百分比
├── status         # Charging / Not charging / Discharging
├── voltage_now    # 电压 (微伏)
└── energy_now     # 能量 (微瓦时)
```

### 6.4 风扇

```
/sys/class/hwmon/hwmon1/
├── name           # "acpi_fan"
├── fan1_input     # 转速 (RPM)
└── fan1_target    # 目标转速 (此 LOQ 型号不可写)
```

### 6.5 服务状态

```bash
systemctl is-active tlp
systemctl is-active nvidia-powerd
```

## 7. CPU 频率控制

### 7.1 sysfs 路径

```
/sys/devices/system/cpu/cpu0/cpufreq/
├── scaling_max_freq           # 当前最大频率
├── scaling_min_freq           # 当前最小频率
├── scaling_governor           # 调频策略
├── energy_performance_preference  # 能效偏好
├── cpuinfo_max_freq           # 硬件最大频率
└── cpuinfo_min_freq           # 硬件最小频率
```

### 7.2 TLP 集成

TLP 通过 `TLP_AUTO_SWITCH` 在 AC/Battery 模式间切换 CPU 设置。
应用功率限制时应考虑 TLP 的影响。

## 8. GUI 架构

### 8.1 组件层次

```
LOQPowerApp (Gtk.Application)
└── LOQPowerWindow (Gtk.ApplicationWindow)
    ├── HeaderBar (刷新/应用按钮)
    ├── PowerPage (Gtk.Box)
    │   ├── ThermalBanner (热模式状态)
    │   ├── ProfileBar (方案选择)
    │   ├── DashboardCard ×3 (GPU/CPU/系统 状态)
    │   ├── PowerCard ×10 (EC 功率滑块)
    │   ├── PowerCard ×1 (CPU 频率)
    │   ├── ComboBox ×3 (调频/能效/平台)
    │   └── ...
    └── StatusBar (Gtk.Label)
```

### 8.2 自动刷新

仪表盘每 3 秒自动刷新 (`GLib.timeout_add_seconds`)，只更新状态数据，不干扰滑块操作。

### 8.3 滚轮事件处理

滑块添加 `EventControllerScroll` (CAPTURE 阶段)，始终返回 False，
让滚轮事件传递给父级 `ScrolledWindow`，避免滚轮被滑块捕获。

## 9. 双系统注意事项

### 9.1 EC 设置持久性

EC 固件设置保存在硬件中，Windows 和 Linux 共享。
在 Windows 上通过 Lenovo Legion Toolkit 设置的参数，重启到 Linux 后仍然有效。

### 9.2 重启后行为

- EC 功率设置: **持久** (重启后保持)
- 热模式: **不持久** (重启后恢复默认 balanced)
- 需要重新应用: 热模式切换 + 功率设置

## 10. 已知问题

### 10.1 风扇曲线不可调

- `acpi_fan` hwmon 的 `fan1_target` 不可写
- `lenovo_wmi_other` 的 fan capdata 在此 LOQ 型号上未注册
- 风扇由 EC 硬件自动控制

### 10.2 acpi_call 无法安装

pacman 镜像缺少 `linux-7.1.4.arch1-1` 包，导致 acpi_call 模块无法安装。

### 10.3 platform_profile 写入

`/sys/firmware/acpi/platform_profile` 写入 "custom" 返回 "Invalid argument"。
必须使用 gamezone WMI 驱动的 platform-profile 接口。

## 11. 调试命令

```bash
# 检查 EC 属性
ls /sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/

# 检查热模式
cat /sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile

# 检查 WMI 设备
ls /sys/bus/wmi/devices/ | grep -i lenovo

# 检查风扇
cat /sys/class/hwmon/hwmon*/fan1_input

# 检查 CPU 频率
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq

# 检查 GPU
nvidia-smi -q

# 测试 EC 写入
echo 60 | sudo tee /sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/gpu_nv_ctgp/current_value

# 压力测试
stress --cpu 16 --timeout 60
```

## 12. 未来开发方向

### 12.1 可能的功能

- [ ] 风扇曲线控制 (需要 acpi_call 或自定义内核模块)
- [ ] 自动配置 (根据 AC/Battery 切换方案)
- [ ] 开机自启动 + 自动应用上次方案
- [ ] 图表显示温度/功耗历史
- [ ] 更多 EC 属性 (如果 firmware 更新暴露更多)

### 12.2 技术探索

- `887B54E2` WMI GUID (object_id=A4) 未绑定驱动，可能是 SetSmartFanMode 接口
- 可以尝试编写内核模块直接调用 WMI 方法
- acpi_call 如果能安装，可以直接调用 ACPI 方法控制风扇

---

*最后更新: 2026-09-10*
