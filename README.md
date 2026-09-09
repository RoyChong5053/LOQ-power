# LOQ Power Control

EC 固件功率管理工具，适用于 Lenovo LOQ 15ARP9 (83JC) 笔记本 (Linux)。

通过 sysfs firmware-attributes 接口直接读写 EC 固件中的功率参数，
支持 GPU cTGP/PPAB、CPU PL1/PL2/PL3、温度限制、CPU 频率控制，
以及实时系统监控仪表盘。

## 截图

```
┌─ 热模式横幅 ─────────────────────────┐
│ ⚪ 白灯 均衡模式  ✓ EC 功率已激活    │
└──────────────────────────────────────┘
┌─ 系统状态 ───────────────────────────┐
│ GPU: 功耗 3.3W / 温度 45°C / 利用率 25% │
│ CPU: 频率 4000MHz / 温度 52°C        │
│ 系统: 风扇 2300RPM / 电池 79%        │
│      充电: 长寿模式 (80%)            │
└──────────────────────────────────────┘
┌─ GPU 功率控制 ───────────────────────┐
│  GPU cTGP     [========] 60W        │
│  可配置总图形功率，影响 GPU 最大功耗上限 │
│  55W                        105W    │
│  GPU PPAB     [========] 25W        │
│  功率加速，GPU 高负载时临时额外功率   │
│  ...                                 │
└──────────────────────────────────────┘
┌─ CPU 功率控制 ───────────────────────┐
│  CPU PL1      [========] 45W        │
│  持续功率限制，长时间负载的最大功耗   │
│  ...                                 │
└──────────────────────────────────────┘
```

## 依赖

- Python 3.10+
- GTK4 + PyGObject (`python-gobject`)
- 内核模块: `lenovo_wmi_other`, `lenovo_wmi_gamezone` (kernel 7.0+ 自带)
- `sudo` 权限 (用于写入 sysfs)

### Arch / CachyOS

```bash
sudo pacman -S python-gobject gtk4
```

## 安装与运行

### 1. 克隆仓库

```bash
git clone https://github.com/RoyChong5053/LOQ-power.git
cd LOQ-power
```

### 2. 安装辅助脚本

```bash
sudo cp loq-power-apply /usr/local/bin/
sudo chmod +x /usr/local/bin/loq-power-apply
```

### 3. 运行

```bash
python3 main.py
```

需要用户有 `sudo NOPASSWD` 权限 (在 `/etc/sudoers` 中配置)。

## 功能

### 系统监控仪表盘 (实时刷新)

- **GPU**: 功耗、温度、利用率、显存使用、EC 功率配置
- **CPU**: 频率、温度、调频策略、能效偏好
- **系统**: 风扇转速、电池状态、电源适配器、TLP/nvidia-powerd 状态

### EC 功率控制

| 参数 | 范围 | 说明 |
|------|------|------|
| GPU cTGP | 55-105W | 可配置总图形功率 |
| GPU PPAB | 10-25W | 功率加速 |
| GPU AC Offset | 10-55W | 电源适配器功率偏移 |
| GPU→CPU Boost | 5-15W | 动态加速 |
| GPU 温度限制 | 75-87°C | GPU 降频温度墙 |
| CPU PL1 | 40-75W | 持续功率 (长时) |
| CPU PL2 | 45-100W | 短时功率 (Turbo Boost) |
| CPU PL3 | 55-125W | 快速功率跟踪 |
| CPU 交叉负载 | 30-45W | GPU 活动时 CPU 功率 |
| CPU 温度限制 | 85-100°C | CPU 降频温度墙 |

每个滑块都有详细的参数说明，帮助理解每个设置的作用。

### CPU 频率控制

- 最大频率限制 (400-5000 MHz)
- 高级选项 (可折叠): 调频策略、能效偏好、平台配置

### 电池管理

- 充电模式切换: Fast (快速充电) / Standard (标准) / Long_Life (长寿80%)
- 实时电池状态: 电量、充电状态、电压

### 热模式横幅

- LED 颜色指示: 🔵蓝灯=省电 ⚪白灯=均衡 🔴红灯=性能 🟣紫灯=极速
- 自动检测 CUSTOM 模式状态
- 显示 EC 功率是否已激活

### 方案管理

- 内置方案: 省电、平衡、性能
- 自定义方案: 保存/加载/切换
- 方案存储: `~/.config/loq-power/profiles.json`

## 关键发现

### Fn+Q vs 软件写入

EC 功率限制写入需要 CUSTOM 热模式 (0xFF)。LOQ 的 Fn+Q 不会切换到 CUSTOM，
但 gamezone WMI 驱动有一个 platform-profile 接口可以直接写入:

```bash
# 查看当前模式
cat /sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile

# 切换到 CUSTOM
echo "custom" | sudo tee /sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile
```

**重要发现:**
- Fn+Q 是硬件中断，同时改变 LED 颜色和风扇策略
- 软件写入是 WMI 调用，只改变 thermal_mode，不影响 LED 和风扇
- Custom 模式下 LED 保持上次 Fn+Q 的颜色，风扇行为也保持不变

### EC vs Windows

Linux 下 EC 控制比 Windows 更稳定:
- Windows 上多个电源管理软件 (Intel DPTF, Lenovo 服务) 互相竞争
- Linux 上只有 `lenovo_wmi_other` 驱动直接写 EC，无干扰
- 温度限制更精确，功率限制更可靠

### 散热对比

| 方案 | 稳定温度 | 稳定频率 | 说明 |
|------|----------|----------|------|
| PL1=45W (默认) | 86-87°C | ~3667 MHz | CPU 被 PPT 自然限制 |
| PL1=40W | 83-84°C | ~3530 MHz | 降 5W，更凉 |
| TLP 锁 4GHz | - | ≤4000 MHz | 减少 boost 波动 |
| EC + TLP 配合 | 更平滑 | ≤4000 MHz | 最佳散热方案 |

## 项目结构

```
LOQ-power/
├── main.py              # 入口
├── ec_backend.py        # EC 读写、CPU/NVIDIA 控制
├── profiles.py          # 方案保存/加载
├── gui/
│   ├── app.py           # GTK4 主窗口
│   ├── power_page.py    # 功率控制页面 + 仪表盘
│   └── widgets.py       # 自定义控件
├── loq-power-apply      # 辅助脚本 (sudo 执行)
└── org.legion-power.policy  # PolicyKit 规则 (已弃用)
```

## 已知限制

- **风扇曲线不可调**: EC 硬件控制，Linux 无接口
- **CUSTOM 模式非永久**: 重启后恢复默认，需重新应用
- **NVIDIA 功率**: nvidia-powerd 管理，EC 设置影响但不直接控制

## License

MIT
