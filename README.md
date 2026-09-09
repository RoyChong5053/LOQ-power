# LOQ Power Control

EC 固件功率管理工具，适用于 Lenovo LOQ 笔记本 (Linux)。

通过 sysfs firmware-attributes 接口直接读写 EC 固件中的功率参数，
支持 GPU cTGP/PPAB、CPU PL1/PL2/PL3、温度限制、CPU 频率控制。

## 依赖

- Python 3.10+
- GTK4 + PyGObject (`python-gobject`)
- 内核模块: `lenovo_wmi_other`, `lenovo_wmi_gamezone` (kernel 7.0+ 自带)

### Arch / CachyOS

```bash
sudo pacman -S python-gobject gtk4
```

## 运行

```bash
python3 main.py
```

写入 EC 设置需要 root 权限，会通过 PolicyKit 弹出密码框。

## 安装 PolicyKit 规则 (可选)

```bash
sudo cp org.legion-power.policy /usr/share/polkit-1/actions/
```

## 功能

- GPU 功率控制: cTGP, PPAB, AC Offset, Dynamic Boost, 温度限制
- CPU 功率控制: PL1, PL2, PL3, 交叉负载功率, 温度限制
- CPU 频率控制: 最大频率, 调频策略, 能效偏好, 平台配置
- 方案管理: 保存/加载/切换预设方案
- 实时状态: NVIDIA GPU 功率读取
