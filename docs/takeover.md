# 从 TLP 接管

TLP 在本机同时运行 `tlp.service` 和 `tlp-pd.service`。LOQ Power 不会在安装时
自动停用它们；接管是一个显式步骤，并且随时可以回滚。

## 为什么需要接管

`tlp-pd` 提供 `net.hadess.PowerProfiles`，桌面电源小组件依赖它。`loq-powerd`
实现了同一接口，因此停用 TLP 后电源模式切换仍然可用。TLP 的 CPU 调频、
PCIe ASPM、WiFi 省电、USB autosuspend 等设置被吸收进配置档的
`power_saving` 段。

## 为什么不会自动接管

安装脚本不会启用 `loq-powerd.service`，仓库也不提供 D-Bus 自动激活文件。
`loq-powerd` 的单元带有 `Conflicts=tlp.service tlp-pd.service`，因此只有显式启动
它才会停用 TLP——而唯一的正常入口就是本页的接管命令或 GUI 向导。

## 三种入口

```bash
# 只看会做什么，不写任何东西
loq-power --dry-run takeover

# 正式接管（需要 root；GUI 中会走 polkit）
sudo loq-power takeover

# 回滚
sudo loq-power restore-tlp
```

GUI 的「系统 → TLP 接管向导」提供同样的操作，通过守护进程执行，polkit 会弹出
认证窗口。

## 接管做了什么

1. 把当前单元状态写入 `/etc/loq-power/takeover.json`：
   每个 TLP 单元是否 `enabled` / `active`，以及 `profiles.toml` 的原内容。
2. 若 `/etc/loq-power/profiles.toml` 不存在，则从当前 TLP 配置导入
   `TLP-AC` / `TLP-BAT` 两套配置档，并把 `auto_switch` 设为 `true`。
   已存在时默认不覆盖（`--force` 可强制）。
3. `systemctl stop` + `disable` `tlp.service` 和 `tlp-pd.service`。
4. `systemctl enable` + `start` `loq-powerd.service`。

任何一步失败都会记录在报告的 `errors` 中；回滚记录在此之前已经写好，所以
不会出现「TLP 停了但无法恢复」的状态。

## 回滚做了什么

1. `stop` + `disable` `loq-powerd.service`。
2. 按 `takeover.json` 恢复每个 TLP 单元的 `enabled` / `active` 状态。
3. 恢复接管前的 `profiles.toml`；若该文件是接管时新建的，则删除。
4. 删除 `takeover.json`。

从守护进程发起回滚时，守护进程先完成文件与服务恢复、答复 D-Bus 调用，
再在 800ms 后停止自己。此时桌面电源小组件可能短暂显示「未知」，TLP 启动后恢复。

## 配置映射

| TLP 键 | LOQ Power |
|---|---|
| `CPU_SCALING_GOVERNOR_ON_{AC,BAT}` | `cpu.governor` |
| `CPU_ENERGY_PERF_POLICY_ON_{AC,BAT}` | `cpu.epp` |
| `CPU_MIN_PERF_ON_{AC,BAT}` / `CPU_MAX_PERF_ON_{AC,BAT}` | `cpu.min_khz` / `cpu.max_khz`（百分比按硬件上限换算） |
| `CPU_BOOST_ON_{AC,BAT}` | `cpu.boost` |
| `PLATFORM_PROFILE_ON_{AC,BAT}` | `platform` |
| `WIFI_PWR_ON_{AC,BAT}` | `power_saving.wifi_power_save` |
| `USB_AUTOSUSPEND_ON_{AC,BAT}` | `power_saving.usb_autosuspend` |
| `PCIE_ASPM_ON_{AC,BAT}` | `power_saving.pcie_aspm` |
| `RUNTIME_PM_ON_{AC,BAT}` | `power_saving.nvme_runtime_pm` |
| `NMI_WATCHDOG` | `power_saving.nmi_watchdog` |
| `SOUND_POWER_SAVE_ON_{AC,BAT}` | `power_saving.audio_power_save` |
| `STOP_CHARGE_THRESH_BAT0` | 100 → `Standard`，80/60/其他 → `Long_Life` |

无法表达的内容（以 10 为步进的磁盘 APM、ThinkPad 专属项、任意充电阈值）会在
导入报告的 `notes` 中列出，不会被静默丢弃。

配置档解析还会按当前硬件做二次校验：频率夹到硬件范围；内核在当前电源状态下
不支持的 governor/EPP（例如插电时只报告 `performance`）会被跳过并给出 warning，
而不是让整次应用失败。

EC 功率限制（`ppt_*`、`gpu_nv_*`、温度墙）在 TLP 中没有对应项，接管后的
`TLP-AC` / `TLP-BAT` 只包含 CPU 与省电设置；四个内置配置档仍然提供完整的
EC 方案。

## USB 排除规则

USB autosuspend 会跳过 `snd-usb-audio` 和 `usblp` 设备，避免爆音和打印中断，
与 TLP 的行为一致。

## 验收清单

- [ ] `loq-power --dry-run takeover` 输出符合预期
- [ ] 接管后 `systemctl is-active tlp tlp-pd` 为 `inactive`
- [ ] 接管后 `systemctl is-active loq-powerd` 为 `active`
- [ ] `busctl --system get-property net.hadess.PowerProfiles /net/hadess/PowerProfiles net.hadess.PowerProfiles ActiveProfile` 有输出
- [ ] 插拔电源后配置档自动切换
- [ ] `sudo loq-power restore-tlp` 后 TLP 恢复原来的启用/运行状态
