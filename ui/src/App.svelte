<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Capabilities } from "./lib/types";

  type Tab = "dashboard" | "power" | "ec" | "system";

  let tab = $state<Tab>("dashboard");
  let capabilities = $state<Capabilities | null>(null);
  let loading = $state(true);
  let error = $state("");

  async function refresh() {
    loading = true;
    error = "";
    try {
      capabilities = await invoke<Capabilities>("get_capabilities");
    } catch (reason) {
      error = String(reason);
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  function number(value: number | null | undefined, digits = 0): string {
    if (value === null || value === undefined || Number.isNaN(value)) return "—";
    return value.toFixed(digits);
  }

  function battery() {
    return capabilities?.power_supply.batteries[0] ?? null;
  }

  function acLabel(): string {
    const ac = capabilities?.power_supply.ac_online;
    if (ac === true) return "AC 通电";
    if (ac === false) return "电池供电";
    return "未知";
  }

  function tlpLabel(): string {
    if (!capabilities?.tlp.installed) return "未安装";
    return capabilities.tlp.service_active ? "运行中" : "已停止";
  }

  function chargeLabel(): string {
    const type = battery()?.charge_type;
    const map: Record<string, string> = {
      Fast: "快速充电",
      Standard: "标准充电",
      Long_Life: "长寿模式 (80%)",
    };
    return type ? (map[type] ?? type) : "—";
  }

  function fanSpeed(): string {
    const fan = capabilities?.hwmon
      .flatMap((device) => device.fans)
      .find((sensor) => sensor.label.toLowerCase().includes("fan"));
    return fan ? `${number(fan.value)} RPM` : "—";
  }

  function cpuTemp(): string {
    const preferred = capabilities?.hwmon.find((device) => device.name === "k10temp");
    const sensor = preferred?.temperatures[0];
    return sensor ? `${number(sensor.value, 1)} °C` : "—";
  }
</script>

<div class="app">
  <aside class="sidebar">
    <div class="brand">LOQ<span>Power</span></div>
    <nav>
      <button class:active={tab === "dashboard"} onclick={() => (tab = "dashboard")}>
        <span class="dot"></span>仪表盘
      </button>
      <button class:active={tab === "power"} onclick={() => (tab = "power")}>
        <span class="dot"></span>功率
      </button>
      <button class:active={tab === "ec"} onclick={() => (tab = "ec")}>
        <span class="dot"></span>EC 浏览器
      </button>
      <button class:active={tab === "system"} onclick={() => (tab = "system")}>
        <span class="dot"></span>系统
      </button>
    </nav>
    <div class="side-footer">Phase 0 · 只读探测</div>
  </aside>

  <main class="content">
    <header class="topbar">
      <div>
        <h1>LOQ Power</h1>
        <p class="subtitle">
          {capabilities?.dmi.sys_vendor ?? "Lenovo"} {capabilities?.dmi.product_name ?? ""}
          · {capabilities?.kernel ?? "kernel 未知"}
        </p>
      </div>
      <button class="primary" onclick={refresh} disabled={loading}>
        {loading ? "读取中…" : "刷新"}
      </button>
    </header>

    {#if error}
      <section class="notice error">
        <strong>无法读取能力清单。</strong>
        <p>{error}</p>
        <p class="hint">请通过 Tauri 启动应用：<code>cargo tauri dev</code>。</p>
      </section>
    {:else if capabilities}
      {#if tab === "dashboard"}
        <section class="grid">
          <article class="card">
            <span class="label">热模式</span>
            <strong>{capabilities.platform.current ?? "未知"}</strong>
            <span class="hint">{capabilities.platform.choices.join(" · ")}</span>
          </article>
          <article class="card">
            <span class="label">电源</span>
            <strong>{acLabel()}</strong>
            <span class="hint">{chargeLabel()}</span>
          </article>
          <article class="card">
            <span class="label">电池</span>
            <strong>{battery()?.capacity ?? "—"}%</strong>
            <span class="hint">{battery()?.status ?? "—"}</span>
          </article>
          <article class="card">
            <span class="label">CPU 温度</span>
            <strong>{cpuTemp()}</strong>
            <span class="hint">{capabilities.cpu.governor ?? "—"} · EPP {capabilities.cpu.epp ?? "—"}</span>
          </article>
          <article class="card">
            <span class="label">GPU</span>
            <strong>{number(capabilities.gpu?.power_draw_w, 1)} W</strong>
            <span class="hint">
              {number(capabilities.gpu?.temperature_c)} °C · 利用率 {number(capabilities.gpu?.utilization_percent)}%
            </span>
          </article>
          <article class="card">
            <span class="label">风扇</span>
            <strong>{fanSpeed()}</strong>
            <span class="hint">EC 风扇模式 {capabilities.ideapad.attributes.find((a) => a.name === "fan_mode")?.value ?? "—"}</span>
          </article>
          <article class="card">
            <span class="label">TLP</span>
            <strong>{tlpLabel()}</strong>
            <span class="hint">{capabilities.tlp.version ?? "—"} · tlp-pd {capabilities.tlp.pd_active ? "运行" : "停止"}</span>
          </article>
          <article class="card">
            <span class="label">EC 属性</span>
            <strong>{capabilities.ec.attributes.length}</strong>
            <span class="hint">kernel 暴露的全部可写功率参数</span>
          </article>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>传感器</h2>
            <span class="hint">{capabilities.hwmon.length} 个 hwmon 设备</span>
          </div>
          <div class="sensor-grid">
            {#each capabilities.hwmon as device}
              {#if device.temperatures.length || device.fans.length || device.powers.length}
                <div class="sensor">
                  <strong>{device.name}</strong>
                  <div class="sensor-values">
                    {#each device.temperatures as sensor}
                      <span>{sensor.label} {number(sensor.value, 1)}°C</span>
                    {/each}
                    {#each device.fans as sensor}
                      <span>{sensor.label} {number(sensor.value)} RPM</span>
                    {/each}
                    {#each device.powers as sensor}
                      <span>{sensor.label} {number(sensor.value, 1)} W</span>
                    {/each}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        </section>
      {:else if tab === "power"}
        <section class="notice">
          <strong>只读模式。</strong>
          写入功能会在 <code>loq-powerd</code> + polkit 完成后开放。
        </section>
        <section class="panel">
          <div class="panel-head">
            <h2>EC 功率</h2>
            <span class="hint">{capabilities.ec.base ?? "未发现 lenovo-wmi-other"}</span>
          </div>
          <div class="slider-list">
            {#each capabilities.ec.attributes as attribute}
              <div class="slider-row">
                <div class="slider-title">
                  <strong>{attribute.display_name}</strong>
                  <span>{attribute.current ?? "—"} {attribute.unit}</span>
                </div>
                <input
                  type="range"
                  min={attribute.min ?? 0}
                  max={attribute.max ?? 100}
                  step={attribute.step ?? 1}
                  value={attribute.current ?? attribute.min ?? 0}
                  disabled
                />
                <div class="slider-meta">
                  <span>{attribute.min ?? "—"}{attribute.unit}</span>
                  <span>默认 {attribute.default ?? "—"}</span>
                  <span>{attribute.max ?? "—"}{attribute.unit}</span>
                </div>
                <p class="hint">{attribute.description}</p>
              </div>
            {/each}
          </div>
        </section>
      {:else if tab === "ec"}
        <section class="panel">
          <div class="panel-head">
            <h2>EC 固件属性</h2>
            <span class="hint">{capabilities.ec.attributes.length} 项</span>
          </div>
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>属性</th>
                  <th>当前</th>
                  <th>范围</th>
                  <th>默认</th>
                  <th>单位</th>
                  <th>路径</th>
                </tr>
              </thead>
              <tbody>
                {#each capabilities.ec.attributes as attribute}
                  <tr>
                    <td>
                      <strong>{attribute.display_name}</strong>
                      <div class="hint">{attribute.name}</div>
                    </td>
                    <td class="mono">{attribute.current ?? "—"}</td>
                    <td class="mono">{attribute.min ?? "?"}..{attribute.max ?? "?"}</td>
                    <td class="mono">{attribute.default ?? "—"}</td>
                    <td>{attribute.unit || "—"}</td>
                    <td class="mono path">{attribute.path}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>Ideapad EC 接口</h2>
            <span class="hint">{capabilities.ideapad.base ?? "未发现"}</span>
          </div>
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>属性</th>
                  <th>值</th>
                  <th>可写</th>
                  <th>说明</th>
                </tr>
              </thead>
              <tbody>
                {#each capabilities.ideapad.attributes as attribute}
                  <tr>
                    <td class="mono">{attribute.name}</td>
                    <td class="mono">{attribute.value ?? "—"}</td>
                    <td>{attribute.writable ? "rw" : "ro"}</td>
                    <td class="hint">{attribute.description || "未识别，保持只读"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>LED</h2>
          </div>
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>名称</th>
                  <th>亮度</th>
                  <th>最大</th>
                </tr>
              </thead>
              <tbody>
                {#each capabilities.leds as led}
                  <tr>
                    <td class="mono">{led.name}</td>
                    <td class="mono">{led.brightness ?? "—"}</td>
                    <td class="mono">{led.max_brightness ?? "—"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>
      {:else if tab === "system"}
        <section class="grid">
          <article class="card">
            <span class="label">CPU</span>
            <strong>{capabilities.cpu.cpus} 核</strong>
            <span class="hint">
              {capabilities.cpu.driver ?? "—"} · {capabilities.cpu.scaling_min_khz ?? "—"}..{capabilities.cpu.scaling_max_khz ?? "—"} kHz
            </span>
          </article>
          <article class="card">
            <span class="label">平台配置</span>
            <strong>{capabilities.platform.current ?? "—"}</strong>
            <span class="hint">choices: {capabilities.platform.choices.join(" ")}</span>
          </article>
          <article class="card">
            <span class="label">TLP</span>
            <strong>{tlpLabel()}</strong>
            <span class="hint">enabled {String(capabilities.tlp.service_enabled)} · tlp-pd {String(capabilities.tlp.pd_active)}</span>
          </article>
          <article class="card">
            <span class="label">Legion Linux</span>
            <strong>{capabilities.legion.installed ? "已检测" : "未安装"}</strong>
            <span class="hint">{capabilities.legion.installed ? "风扇曲线可用" : "风扇曲线不可用"}</span>
          </article>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>TLP 生效配置</h2>
            <span class="hint">{capabilities.tlp.config_files.join(", ") || "未发现配置文件"}</span>
          </div>
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>键</th>
                  <th>值</th>
                </tr>
              </thead>
              <tbody>
                {#each Object.entries(capabilities.tlp.effective) as [key, value]}
                  <tr>
                    <td class="mono">{key}</td>
                    <td class="mono">{value}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>服务</h2>
          </div>
          <div class="service-grid">
            <span>TLP: {String(capabilities.services.tlp)}</span>
            <span>tlp-pd: {String(capabilities.services.tlp_pd)}</span>
            <span>nvidia-powerd: {String(capabilities.services.nvidia_powerd)}</span>
            <span>power-profiles-daemon: {String(capabilities.services.power_profiles_daemon)}</span>
          </div>
        </section>
      {/if}
    {:else}
      <section class="notice">正在读取硬件能力…</section>
    {/if}
  </main>
</div>
