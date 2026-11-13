# 架构

LOQ Power 由一个特权守护进程、一个非特权 CLI/GUI 和一小组打包文件组成。所有
`/sys` 与 `/proc` 写入都集中在 `loq-core`，调用方不能指定路径。

```
┌────────────┐   D-Bus    ┌──────────────┐   sysfs   ┌────────────┐
│ loq-gui    │──────────▶│              │──────────▶│ kernel / EC│
│ loq-power  │  org.loq.  │  loq-powerd  │           └────────────┘
└────────────┘  Power1    │  (root)      │
      │                   │              │
      │ pkexec 回退        │ polkit 鉴权   │
      ▼                   └──────────────┘
┌────────────────┐
│ loq-power-helper│  ← 仅在守护进程未运行时使用
└────────────────┘
```

## Crate 划分

| Crate | 作用 |
|---|---|
| `loq-core` | 能力探测、sysfs 读写、校验、配置档解析、TLP 导入、接管逻辑 |
| `loq-cli` | `loq-power` 命令行：`probe`/`status`/`ec`/`platform`/`charge`/`fan`/`cpu`/`led`/`profile`/`tlp`/`takeover`/`restore-tlp`/`apply` |
| `loq-helper` | root 权限助手，stdin 接收 `ApplyRequest`，也支持 `--takeover` / `--restore-tlp` |
| `loq-powerd` | 系统守护进程：D-Bus 接口、polkit 鉴权、AC/BAT 自动切换、唤醒后重放、PPD 兼容 |
| `loq-gui` | Tauri 2 外壳，前端在 `ui/`（Svelte 5 + Vite + TypeScript） |
| `ui/` | 仪表盘、功率、风扇/电池、省电、系统、EC 浏览器、TLP 接管向导 |

## 写入安全模型

1. 调用方只提交**意图**（属性名 + 值），不提交路径。
2. `loq-core` 在实时 sysfs 树上重新发现路径，并校验范围、步进和可选项。
3. EC 属性写入前自动把热模式切到 `custom`（以 `balanced` 为风扇基底），
   写完后由上层恢复目标热模式。
4. 每次写入都回读验证；失败返回 `Verification` 错误。
5. 值未变化时跳过（`skipped`），避免无意义的 EC 写入。
6. `--dry-run` 不写任何东西，且不需要 root。

### 特权路径

- CLI 检测到 `org.loq.Power1` 在系统总线上时，优先走守护进程；
- 守护进程未运行且当前不是 root 时，通过 `pkexec loq-power-helper` 执行；
- 守护进程对每个修改方法调用 polkit 的 `CheckAuthorization`，动作 ID 见下。

## D-Bus 接口

服务名 `org.loq.Power1`，对象 `/org/loq/Power1`，接口 `org.loq.Power1`。

### 方法

| 方法 | 说明 |
|---|---|
| `GetState() -> s` | 状态快照 JSON |
| `Probe() -> s` | 完整能力清单 JSON |
| `ApplyProfile(name: s, dry_run: b) -> s` | 应用命名配置档 |
| `Apply(request_json: s) -> s` | 应用任意 `ApplyRequest` |
| `SetEc(values: a{si}, dry_run: b) -> s` | 写一个或多个 EC 属性 |
| `SetPlatform(mode: s) -> s` | 切换热模式 |
| `SetChargeType(charge_type: s) -> s` | 切换充电类型 |
| `SetFanMode(mode: u) -> s` | 切换 EC 风扇模式 |
| `GetTakeoverStatus() -> s` | TLP 接管状态与回滚记录 |
| `Takeover(import: b, force: b) -> s` | 停用 TLP，启用并启动守护进程 |
| `RestoreTlp() -> s` | 还原 TLP；守护进程随后自行停止 |
| `Reload() -> s` | 重新读取 `/etc/loq-power/profiles.toml` |

### 属性与信号

- 属性：`ActiveProfile`、`PowerSource`（`ac`/`battery`/`unknown`）、
  `ThermalMode`、`DaemonVersion`
- 信号：`StateChanged(s)`；属性变化同时发送标准 `PropertiesChanged`

### `net.hadess.PowerProfiles` 兼容层

对象 `/net/hadess/PowerProfiles`，接口 `net.hadess.PowerProfiles`：

- 属性：`ActiveProfile`、`PerformanceDegraded`、`Profiles`、`Actions`、
  `ActiveProfileHolds`
- 方法：`HoldProfile(profile, reason, application_id) -> u`、`ReleaseProfile(cookie)`
- 三档映射：`power-saver → low-power`、`balanced → balanced`、
  `performance → performance`（`max-power` 通过 LOQ 接口设置）

这样停用 `tlp-pd` 后，GNOME/KDE 的电源模式小组件仍然可用。

## polkit 动作

| 动作 | 用途 | 默认 |
|---|---|---|
| `org.loq.power.read` | 只读状态 | 允许 |
| `org.loq.power.apply` | 应用设置、切换配置档 | `auth_admin_keep` |
| `org.loq.power.helper` | pkexec 启动助手 | `auth_admin_keep`，绑定 `/usr/bin/loq-power-helper` |
| `org.loq.power.takeover` | TLP 接管/还原 | `auth_admin` |
| `org.loq.power.fan-curve` | 风扇曲线（LLL，预留） | `auth_admin` |
| `org.loq.power.raw-ec` | 原始 EC 访问（预留） | `auth_admin` |

## 自动切换

- `[power] auto_switch` 默认为 `true`（可在文件中设为 `false` 关闭）。
- 守护进程每 3 秒轮询 `AC` 在线状态；变化时应用 `[power]` 中映射的配置档。
- 若启动时 TLP 仍在运行，守护进程进入 standby：只提供只读状态与显式操作，
  不自动应用配置档，直到接管向导清除该状态或重启守护进程。
- 启动时立即断言当前电源来源对应的配置档，避免带着上次的状态重启。
- 通过 logind 的 `PrepareForSleep(false)` 检测唤醒，等待 2 秒后重新应用活动配置档，
  以修复挂起后可能被固件重置的 EC / CPU 设置。

## 配置

- 系统：`/etc/loq-power/profiles.toml`（守护进程使用），示例见
  [`packaging/profiles.toml`](../packaging/profiles.toml)
- 用户：`~/.config/loq-power/profiles.toml`（CLI/`profile` 子命令使用）
- 覆盖：`LOQ_POWER_CONFIG`（守护进程）、`LOQ_POWER_PROFILES`（CLI）
- 文件不存在时使用内置的四套配置档：省电 / 平衡 / 性能 / 自定义
