<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Capabilities, FanCurve } from "./lib/types";

  type Tab = "dashboard" | "power" | "fan" | "system" | "ec";

  interface DaemonState {
    active_profile: string;
    power_source: string;
    thermal_mode: string;
    ppd_profile: string;
    holds: number;
  }

  interface TakeoverStatus {
    taken_over: boolean;
    rollback_available: boolean;
    tlp: {
      installed: boolean;
      service_active: boolean | null;
      pd_active: boolean | null;
      config_files: string[];
    };
  }

  interface TakeoverReport {
    dry_run: boolean;
    steps: { action: string; ok: boolean; detail: string }[];
    notes: string[];
    errors: string[];
  }

  interface ApplyResponse {
    ok: boolean;
    dry_run: boolean;
    writes: { target: string; previous: string | null; value: string; verified: boolean }[];
    skipped: string[];
    errors: string[];
    warnings?: string[];
  }

  let tab = $state<Tab>("dashboard");
  let capabilities = $state<Capabilities | null>(null);
  let daemon = $state<DaemonState | null>(null);
  let daemonChecked = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let notice = $state("");
  let noticeKind = $state<"ok" | "error" | "info">("info");

  let ecDraft = $state<Record<string, number>>({});
  let platformTarget = $state("");
  let dirty = $state(false);
  let takeoverStatus = $state<TakeoverStatus | null>(null);
  let wizardBusy = $state(false);
  let curveDraft = $state<FanCurve | null>(null);

  async function refresh(silent = false) {
    if (!silent) loading = true;
    error = "";
    try {
      const next = await invoke<Capabilities>("get_capabilities");
      capabilities = next;

      if (!dirty || !silent) {
        const draft: Record<string, number> = {};
        for (const attribute of next.ec.attributes) {
          draft[attribute.name] = attribute.current ?? attribute.min ?? 0;
        }
        ecDraft = draft;
        platformTarget = next.platform.current ?? "";
        dirty = false;
        if (next.legion.fan_curve) {
          curveDraft = structuredClone(next.legion.fan_curve);
        } else {
          curveDraft = null;
        }
      }
    } catch (reason) {
      error = String(reason);
    } finally {
      loading = false;
    }
  }

  async function loadDaemon() {
    try {
      daemon = await invoke<DaemonState | null>("get_daemon_state");
    } catch (reason) {
      daemon = null;
      error = String(reason);
    } finally {
      daemonChecked = true;
    }
  }

  async function loadTakeover() {
    try {
      takeoverStatus = await invoke<TakeoverStatus>("get_takeover_status");
    } catch {
      takeoverStatus = null;
    }
  }

  onMount(async () => {
    await refresh();
    await loadDaemon();
    await loadTakeover();
  });

  $effect(() => {
    const id = setInterval(() => {
      if (tab === "dashboard") {
        refresh(true);
        loadDaemon();
      }
    }, 4000);
    return () => clearInterval(id);
  });

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

  function daemonLabel(): string {
    if (!daemonChecked) return "检测中…";
    return daemon ? "运行中" : "未运行";
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

  function thermalLabel(mode: string | null | undefined): string {
    const map: Record<string, string> = {
      "low-power": "静音",
      balanced: "均衡",
      performance: "性能",
      "max-power": "极速",
      custom: "自定义",
    };
    return mode ? (map[mode] ?? mode) : "—";
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

  function fanMode(): number | null {
    const raw = capabilities?.ideapad.attributes.find((attribute) => attribute.name === "fan_mode")?.value;
    if (raw === null || raw === undefined) return null;
    const parsed = Number.parseInt(raw, 10);
    return Number.isNaN(parsed) ? null : parsed;
  }

  function kbdBrightness(): number {
    const led = capabilities?.leds.find((entry) => entry.name.includes("kbd_backlight"));
    return led?.brightness ?? 0;
  }

  function kbdMax(): number {
    const led = capabilities?.leds.find((entry) => entry.name.includes("kbd_backlight"));
    return led?.max_brightness ?? 2;
  }

  function markDirty() {
    dirty = true;
  }

  function formatResponse(response: ApplyResponse): void {
    if (response.ok) {
      const writes = response.writes.length;
      const skipped = response.skipped.length;
      noticeKind = "ok";
      notice = response.dry_run
        ? `预览通过：将写入 ${writes} 项${skipped ? `，跳过 ${skipped} 项` : ""}。`
        : `已应用 ${writes} 项${skipped ? `，跳过 ${skipped} 项` : ""}。`;
    } else {
      noticeKind = "error";
      notice = `失败：${response.errors.join("；")}`;
    }
  }

  function resetDrafts() {
    if (!capabilities) return;
    const draft: Record<string, number> = {};
    for (const attribute of capabilities.ec.attributes) {
      draft[attribute.name] = attribute.current ?? attribute.min ?? 0;
    }
    ecDraft = draft;
    platformTarget = capabilities.platform.current ?? "";
    curveDraft = capabilities.legion.fan_curve
      ? structuredClone(capabilities.legion.fan_curve)
      : null;
    dirty = false;
  }

  function buildRequest(dryRun: boolean) {
    if (!capabilities) return null;
    const ec: Record<string, number> = {};
    for (const attribute of capabilities.ec.attributes) {
      const value = ecDraft[attribute.name];
      if (value !== undefined && value !== attribute.current) {
        ec[attribute.name] = value;
      }
    }

    const request: Record<string, unknown> = { dry_run: dryRun };
    if (Object.keys(ec).length > 0) request.ec = ec;
    if (platformTarget && platformTarget !== capabilities.platform.current) {
      request.platform = platformTarget;
    }
    return request;
  }

  async function applyEc(dryRun: boolean) {
    const request = buildRequest(dryRun);
    if (!request) return;
    if (Object.keys(request).length === 1) {
      noticeKind = "info";
      notice = "没有需要应用的更改。";
      return;
    }
    await send(request);
  }

  function formatTakeover(report: TakeoverReport, verb: string): void {
    const failed = report.errors.length > 0;
    noticeKind = failed ? "error" : "ok";
    const steps = report.steps.map((step) => `${step.ok ? "✓" : "×"} ${step.action}`).join("；");
    notice = failed
      ? `${verb}失败：${report.errors.join("；")}`
      : `${verb}完成：${steps}`;
    if (report.notes.length > 0) {
      notice += `（${report.notes.join("；")}）`;
    }
  }

  async function startTakeover() {
    if (
      !confirm(
        "接管将停止并禁用 TLP（tlp.service / tlp-pd.service），导入 TLP 配置并启动 loq-powerd。\n\n继续吗？",
      )
    ) {
      return;
    }
    wizardBusy = true;
    notice = "";
    try {
      const report = await invoke<TakeoverReport>("takeover", { import: true, force: false });
      formatTakeover(report, "TLP 接管");
      await refresh();
      await loadDaemon();
      await loadTakeover();
    } catch (reason) {
      noticeKind = "error";
      notice = String(reason);
    } finally {
      wizardBusy = false;
    }
  }

  async function restoreTlp() {
    if (!confirm("将禁用 loq-powerd 并恢复接管前的 TLP 状态。\n\n继续吗？")) return;
    wizardBusy = true;
    notice = "";
    try {
      const report = await invoke<TakeoverReport>("restore_tlp");
      formatTakeover(report, "TLP 还原");
      await refresh();
      await loadDaemon();
      await loadTakeover();
    } catch (reason) {
      noticeKind = "error";
      notice = String(reason);
    } finally {
      wizardBusy = false;
    }
  }

  async function send(request: Record<string, unknown>) {
    busy = true;
    notice = "";
    try {
      const response = await invoke<ApplyResponse>("apply_request", {
        requestJson: JSON.stringify(request),
      });
      formatResponse(response);
      if (response.ok && !response.dry_run) {
        await refresh();
        await loadDaemon();
      }
    } catch (reason) {
      noticeKind = "error";
      notice = String(reason);
    } finally {
      busy = false;
    }
  }

  async function setFanMode(mode: number, label: string) {
    if ((mode === 2 || mode === 4) && !confirm(`风扇模式「${label}」会明显变吵，确定要切换吗？`)) {
      return;
    }
    await send({ ideapad: { fan_mode: mode } });
  }

  async function setChargeType(type: string) {
    await send({ battery: { charge_type: type } });
  }

  async function setLegionFullspeed(enabled: boolean) {
    await send({ legion: { fullspeed: enabled } });
  }

  async function setLegionPowermode(mode: number) {
    await send({ legion: { powermode: mode } });
  }

  async function applyCurve() {
    if (!curveDraft) return;
    if (!confirm("将把整条风扇曲线写入 EC，过程会短暂改变风扇转速。\n\n继续吗？")) return;
    await send({ legion: { fan_curve: curveDraft } });
  }

  function curveValue(group: "fan1_pwm" | "fan2_pwm" | "cpu_temp" | "gpu_temp" | "ic_temp", index: number): string {
    const values = curveDraft?.[group];
    if (!values || values[index] === undefined) return "";
    return String(values[index]);
  }

  function setCurveValue(
    group: "fan1_pwm" | "fan2_pwm" | "cpu_temp" | "gpu_temp" | "ic_temp",
    index: number,
    raw: string,
  ) {
    if (!curveDraft) return;
    const values = curveDraft[group];
    if (!values) return;
    const parsed = Number.parseInt(raw, 10);
    if (Number.isNaN(parsed)) return;
    values[index] = parsed;
    curveDraft = { ...curveDraft };
  }

  const legionPowermodes = [
    { mode: 0, label: "均衡" },
    { mode: 1, label: "性能" },
    { mode: 2, label: "安静" },
  ];

  async function setBacklight(value: number) {
    await send({ led: { name: "platform::kbd_backlight", brightness: value } });
  }

  const fanModes = [
    { mode: 0, label: "静音", hint: "Super Silent" },
    { mode: 1, label: "标准", hint: "Standard" },
    { mode: 2, label: "除尘", hint: "Dust Cleaning · 会变吵" },
    { mode: 4, label: "强散热", hint: "Full Speed · 会变吵" },
  ];
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
      <button class:active={tab === "fan"} onclick={() => (tab = "fan")}>
        <span class="dot"></span>风扇 / 电池
      </button>
      <button class:active={tab === "system"} onclick={() => (tab = "system")}>
        <span class="dot"></span>系统
      </button>
      <button class:active={tab === "ec"} onclick={() => (tab = "ec")}>
        <span class="dot"></span>EC 浏览器
      </button>
    </nav>
    <div class="side-footer">
      <div class="daemon-state" class:online={daemon}>
        {daemonLabel()}
      </div>
      <div>Phase 4 · 可读写</div>
    </div>
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
      <div class="actions">
        <button class="ghost" onclick={() => { refresh(); loadDaemon(); loadTakeover(); }} disabled={loading || busy}>
          {loading ? "读取中…" : "刷新"}
        </button>
      </div>
    </header>

    {#if error}
      <section class="notice error">
        <strong>无法读取能力清单。</strong>
        <p>{error}</p>
        <p class="hint">请通过 Tauri 启动应用：<code>cargo tauri dev</code>。</p>
      </section>
    {/if}

    {#if notice}
      <section class="notice" class:error={noticeKind === "error"} class:ok={noticeKind === "ok"}>
        <p>{notice}</p>
        <button class="dismiss" onclick={() => (notice = "")}>关闭</button>
      </section>
    {/if}

    {#if capabilities}
      {#if tab === "dashboard"}
        <section class="grid">
          <article class="card">
            <span class="label">热模式</span>
            <strong>{thermalLabel(capabilities.platform.current)}</strong>
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
            <span class="hint">EC 风扇模式 {fanMode() ?? "—"}</span>
          </article>
          <article class="card">
            <span class="label">守护进程</span>
            <strong>{daemonLabel()}</strong>
            <span class="hint">{daemon ? `PPD ${daemon.ppd_profile} · holds ${daemon.holds}` : "使用 pkexec 回退"}</span>
          </article>
          <article class="card">
            <span class="label">TLP</span>
            <strong>{tlpLabel()}</strong>
            <span class="hint">{capabilities.tlp.version ?? "—"} · tlp-pd {capabilities.tlp.pd_active ? "运行" : "停止"}</span>
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
        <section class="panel">
          <div class="panel-head">
            <h2>热模式</h2>
            <span class="hint">EC 写入需要 CUSTOM，应用时会自动切换并恢复目标模式</span>
          </div>
          <div class="mode-row">
            {#each capabilities.platform.choices as mode}
              <button
                class="mode"
                class:active={platformTarget === mode}
                onclick={() => { platformTarget = mode; markDirty(); }}
              >
                {thermalLabel(mode)}
                <span>{mode}</span>
              </button>
            {/each}
          </div>
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
                  <span class="value">
                    {ecDraft[attribute.name] ?? attribute.current ?? "—"} {attribute.unit}
                    {#if attribute.current !== null && ecDraft[attribute.name] !== attribute.current}
                      <em>（当前 {attribute.current}）</em>
                    {/if}
                  </span>
                </div>
                <input
                  type="range"
                  min={attribute.min ?? 0}
                  max={attribute.max ?? 100}
                  step={attribute.step ?? 1}
                  bind:value={ecDraft[attribute.name]}
                  oninput={markDirty}
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
          <div class="panel-actions">
            <button class="ghost" onclick={resetDrafts} disabled={busy}>还原</button>
            <button class="ghost" onclick={() => applyEc(true)} disabled={busy}>预览</button>
            <button class="primary" onclick={() => applyEc(false)} disabled={busy}>
              {busy ? "应用中…" : "应用"}
            </button>
          </div>
        </section>
      {:else if tab === "fan"}
        <section class="panel">
          <div class="panel-head">
            <h2>EC 风扇模式</h2>
            <span class="hint">读回值可能不可靠；重启后不保留</span>
          </div>
          <div class="mode-row">
            {#each fanModes as entry}
              <button
                class="mode"
                class:active={fanMode() === entry.mode}
                onclick={() => setFanMode(entry.mode, entry.label)}
                disabled={busy}
              >
                {entry.label}
                <span>{entry.hint}</span>
              </button>
            {/each}
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>电池充电</h2>
            <span class="hint">当前 {battery()?.charge_type ?? "—"}</span>
          </div>
          <div class="mode-row">
            {#each battery()?.charge_types ?? [] as type}
              <button
                class="mode"
                class:active={battery()?.charge_type === type}
                onclick={() => setChargeType(type)}
                disabled={busy}
              >
                {type === "Fast" ? "快速" : type === "Standard" ? "标准" : type === "Long_Life" ? "长寿" : type}
                <span>{type}</span>
              </button>
            {/each}
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>键盘背光</h2>
            <span class="hint">0 关闭 · 1 低 · 2 高</span>
          </div>
          <div class="slider-row">
            <input
              type="range"
              min="0"
              max={kbdMax()}
              step="1"
              value={kbdBrightness()}
              onchange={(event) => setBacklight(Number((event.currentTarget as HTMLInputElement).value))}
              disabled={busy}
            />
          </div>
        </section>

        {#if capabilities.legion.installed}
          <section class="panel">
            <div class="panel-head">
              <h2>LenovoLegionLinux</h2>
              <span class="hint">检测到 legion_hwmon</span>
            </div>
            <div class="sensor-grid">
              {#each capabilities.legion.fans as sensor}
                <div class="sensor"><strong>{sensor.label}</strong><span>{number(sensor.value)} {sensor.unit}</span></div>
              {/each}
            </div>
            <div class="mode-row">
              <label class="toggle">
                <input
                  type="checkbox"
                  checked={capabilities.legion.fullspeed === true}
                  onchange={(event) => setLegionFullspeed((event.currentTarget as HTMLInputElement).checked)}
                  disabled={busy || !capabilities.legion.fullspeed_path}
                />
                风扇全速
              </label>
              {#each legionPowermodes as entry}
                <button
                  class="mode"
                  class:active={Number(capabilities.legion.powermode) === entry.mode}
                  onclick={() => setLegionPowermode(entry.mode)}
                  disabled={busy || !capabilities.legion.powermode_path}
                >
                  {entry.label}
                  <span>powermode {entry.mode}</span>
                </button>
              {/each}
            </div>
          </section>

          <section class="panel">
            <div class="panel-head">
              <h2>风扇曲线</h2>
              <span class="hint">
                {curveDraft ? `${curveDraft.points} 个点 · 通过 hwmon 写入` : "本机未暴露 hwmon 风扇曲线"}
              </span>
            </div>
            {#if curveDraft}
              <div class="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>点</th>
                      <th>风扇 1 PWM</th>
                      {#if curveDraft.fan2_pwm}<th>风扇 2 PWM</th>{/if}
                      <th>CPU °C</th>
                      {#if curveDraft.gpu_temp}<th>GPU °C</th>{/if}
                      {#if curveDraft.ic_temp}<th>IC °C</th>{/if}
                    </tr>
                  </thead>
                  <tbody>
                    {#each Array.from({ length: curveDraft.points }, (_, index) => index) as index}
                      <tr>
                        <td class="mono">{index + 1}</td>
                        <td>
                          <input
                            class="cell-input"
                            type="number"
                            min="0"
                            max="255"
                            value={curveValue("fan1_pwm", index)}
                            onchange={(event) => setCurveValue("fan1_pwm", index, (event.currentTarget as HTMLInputElement).value)}
                          />
                        </td>
                        {#if curveDraft.fan2_pwm}
                          <td>
                            <input
                              class="cell-input"
                              type="number"
                              min="0"
                              max="255"
                              value={curveValue("fan2_pwm", index)}
                              onchange={(event) => setCurveValue("fan2_pwm", index, (event.currentTarget as HTMLInputElement).value)}
                            />
                          </td>
                        {/if}
                        <td>
                          <input
                            class="cell-input"
                            type="number"
                            min="0"
                            max="127"
                            value={curveValue("cpu_temp", index)}
                            onchange={(event) => setCurveValue("cpu_temp", index, (event.currentTarget as HTMLInputElement).value)}
                          />
                        </td>
                        {#if curveDraft.gpu_temp}
                          <td>
                            <input
                              class="cell-input"
                              type="number"
                              min="0"
                              max="127"
                              value={curveValue("gpu_temp", index)}
                              onchange={(event) => setCurveValue("gpu_temp", index, (event.currentTarget as HTMLInputElement).value)}
                            />
                          </td>
                        {/if}
                        {#if curveDraft.ic_temp}
                          <td>
                            <input
                              class="cell-input"
                              type="number"
                              min="0"
                              max="127"
                              value={curveValue("ic_temp", index)}
                              onchange={(event) => setCurveValue("ic_temp", index, (event.currentTarget as HTMLInputElement).value)}
                            />
                          </td>
                        {/if}
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              <div class="panel-actions">
                <button
                  class="ghost"
                  onclick={() => (curveDraft = capabilities?.legion.fan_curve ? structuredClone(capabilities.legion.fan_curve) : null)}
                  disabled={busy}
                >
                  还原
                </button>
                <button class="primary" onclick={applyCurve} disabled={busy}>
                  {busy ? "写入中…" : "写入曲线"}
                </button>
              </div>
              <p class="hint curve-note">
                温度与转速必须随点号单调不减。部分机型会把 PWM 四舍五入到 0–10 档；切换 LLL 电源模式或重启后
                固件可能恢复默认曲线，需要重新写入。
              </p>
            {:else}
              <p class="hint curve-note">
                legion_hwmon 已加载，但没有 pwm*_auto_point* 属性；该机型的 LLL 驱动可能尚未支持风扇曲线写入。
              </p>
            {/if}
          </section>
        {:else}
          <section class="notice">
            未检测到 LenovoLegionLinux。安装树外模块后可在这里编辑风扇曲线；本工具不会自动加载它。
          </section>
        {/if}
      {:else if tab === "system"}
        <section class="grid">
          <article class="card">
            <span class="label">守护进程</span>
            <strong>{daemonLabel()}</strong>
            <span class="hint">{daemon ? `配置档 ${daemon.active_profile}` : "启动 loq-powerd 以启用自动切换"}</span>
          </article>
          <article class="card">
            <span class="label">电源</span>
            <strong>{acLabel()}</strong>
            <span class="hint">自动切换 {capabilities.tlp.effective.AUTO_SWITCH ?? "由 loq-powerd 配置决定"}</span>
          </article>
          <article class="card">
            <span class="label">CPU</span>
            <strong>{capabilities.cpu.cpus} 核</strong>
            <span class="hint">
              {capabilities.cpu.driver ?? "—"} · {capabilities.cpu.scaling_min_khz ?? "—"}..{capabilities.cpu.scaling_max_khz ?? "—"} kHz
            </span>
          </article>
          <article class="card">
            <span class="label">TLP</span>
            <strong>{tlpLabel()}</strong>
            <span class="hint">tlp-pd {capabilities.tlp.pd_active ? "运行" : "停止"}</span>
          </article>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>TLP 接管向导</h2>
            <span class="hint">
              {takeoverStatus?.taken_over
                ? "已由 LOQ Power 接管"
                : takeoverStatus?.rollback_available
                  ? "存在可用的回滚记录"
                  : "当前由 TLP 管理"}
            </span>
          </div>
          <div class="wizard">
            <ol class="wizard-steps">
              <li class:done={takeoverStatus?.rollback_available}>
                <strong>1. 保存回滚记录</strong>
                <span>写入 /etc/loq-power/takeover.json，记录 TLP 单元状态与配置备份</span>
              </li>
              <li class:done={takeoverStatus?.taken_over}>
                <strong>2. 导入 TLP 配置</strong>
                <span>把 CPU 调频、PCIe ASPM、WiFi 省电等映射为 LOQ Power 配置档</span>
              </li>
              <li class:done={takeoverStatus?.taken_over}>
                <strong>3. 交接服务</strong>
                <span>停止并禁用 tlp / tlp-pd，启用并启动 loq-powerd</span>
              </li>
            </ol>
            <div class="wizard-actions">
              <button class="ghost" onclick={restoreTlp} disabled={wizardBusy || !takeoverStatus?.rollback_available}>
                还原 TLP
              </button>
              <button class="primary" onclick={startTakeover} disabled={wizardBusy || takeoverStatus?.taken_over}>
                {wizardBusy ? "处理中…" : "开始接管"}
              </button>
            </div>
            <p class="hint">
              接管需要 loq-powerd 正在运行，并由 polkit 授权。TLP 配置不会被删除，随时可还原。
            </p>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h2>TLP 状态</h2>
            <span class="hint">{capabilities.tlp.config_files.join(", ") || "未发现配置文件"}</span>
          </div>
          <div class="table-wrap">
            <table>
              <thead>
                <tr><th>键</th><th>值</th></tr>
              </thead>
              <tbody>
                {#each Object.entries(capabilities.tlp.effective).filter(([key]) =>
                  key.startsWith("CPU_") ||
                  key.startsWith("PLATFORM_PROFILE") ||
                  key.startsWith("STOP_CHARGE") ||
                  key.startsWith("PCIE_ASPM") ||
                  key.startsWith("WIFI_PWR") ||
                  key.startsWith("USB_")
                ) as [key, value]}
                  <tr><td class="mono">{key}</td><td class="mono">{value}</td></tr>
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
            <span>tlp: {String(capabilities.services.tlp)}</span>
            <span>tlp-pd: {String(capabilities.services.tlp_pd)}</span>
            <span>nvidia-powerd: {String(capabilities.services.nvidia_powerd)}</span>
            <span>power-profiles-daemon: {String(capabilities.services.power_profiles_daemon)}</span>
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
                <tr><th>属性</th><th>值</th><th>可写</th><th>说明</th></tr>
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
                <tr><th>名称</th><th>亮度</th><th>最大</th></tr>
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
      {/if}
    {:else}
      <section class="notice">正在读取硬件能力…</section>
    {/if}
  </main>
</div>
