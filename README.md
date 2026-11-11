# LOQ Power

Lenovo LOQ 15ARP9 (83JC) 的 Linux 电源与 EC 控制工具。

> **状态：Rust 重写进行中。** 旧的 Python/GTK4 实现已归档到 [`legacy/`](legacy/)，
> 完整的重写方案见 [`plan.md`](plan.md)。

## 目标

- Rust + Tauri 2 + Svelte 5 重写现有 GUI 与后端
- 覆盖内核已暴露的 EC 功率、热模式、电池充电、风扇模式、键盘背光等能力
- 吸收 TLP 在本机真正生效的 CPU / 省电功能
- 由 `loq-powerd` 作为唯一的电源管理守护进程，支持 AC/BAT 自动切换
- 可选集成 LenovoLegionLinux 风扇曲线（检测到才启用）

## 目录

```
crates/        Rust workspace（开发中）
ui/            Tauri 前端（开发中）
packaging/     systemd / polkit / PKGBUILD（开发中）
docs/          架构与能力文档（开发中）
legacy/        旧 Python 实现（参考与回滚）
plan.md        完整重写计划
```

## 旧版运行方式

```bash
cd legacy
python3 main.py
```

旧版依赖与使用说明见 [`legacy/README.python.md`](legacy/README.python.md)。

## 文档

- [重写计划](plan.md)
- [旧版开发笔记](legacy/DEVELOPMENT.md)
