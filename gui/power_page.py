"""Power control page with dashboard, GPU/CPU sliders and status info."""

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib
from gui.widgets import (PowerCard, SectionHeader, StatusRow, ProfileButton,
                         DashboardCard, ThermalBanner)
import ec_backend
import profiles as prof


# EC attribute descriptions for slider tooltips
EC_DESCRIPTIONS = {
    "gpu_nv_ctgp": "可配置总图形功率，影响 GPU 最大功耗上限。提高可增加 GPU 性能，降低可减少发热",
    "gpu_nv_ppab": "功率加速，在 GPU 高负载时临时额外分配的功率。数值越大，短时性能爆发越强",
    "gpu_nv_ac_offset": "电源适配器接入时的功率偏移。接电源时可分配更多功率给 GPU",
    "gpu_nv_cpu_boost": "GPU 活跃时动态分配给 CPU 的额外功率。游戏时提升 CPU 性能",
    "gpu_temp": "GPU 温度墙，达到此温度后 GPU 开始降频。降低可保护 GPU 但影响持续性能",
    "ppt_pl1_spl": "CPU 持续功率限制，长时间负载的最大功耗。降低可减少发热和风扇噪音",
    "ppt_pl2_sppt": "CPU 短时功率限制，Turbo Boost 时的最大功耗。允许短时间更高性能",
    "ppt_pl3_fppt": "CPU 快速功率跟踪，最短时间内的峰值功率。影响单线程突发性能",
    "ppt_cpu_cl": "GPU 活跃时的 CPU 功率限制。游戏时控制 CPU 功耗分配",
    "cpu_temp": "CPU 温度墙，达到此温度后 CPU 开始降频。降低可减少发热但影响持续性能",
}


class PowerPage(Gtk.Box):
    """Main power control page."""

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.set_vexpand(True)

        self.ec_data = {}
        self.cpu_data = {}
        self.nvidia_data = {}
        self.cards = {}
        self.profile_buttons = {}
        self._active_profile = None
        self._loading = False
        self._auto_refresh_id = None

        # Scrollable content
        scroll = Gtk.ScrolledWindow()
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.append(scroll)

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        scroll.set_child(content)

        # ── Thermal mode banner (new colored banner) ──
        self.thermal_banner = ThermalBanner()
        content.append(self.thermal_banner)

        # ── Profile bar ──
        profile_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        profile_bar.set_margin_start(12)
        profile_bar.set_margin_end(12)
        profile_bar.set_margin_top(4)
        profile_bar.set_margin_bottom(8)
        content.append(profile_bar)

        profile_label = Gtk.Label(label="方案:", xalign=0)
        profile_label.add_css_class("dim-label")
        profile_bar.append(profile_label)

        self.profile_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        profile_bar.append(self.profile_box)

        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        profile_bar.append(spacer)

        self.save_btn = Gtk.Button(label="保存方案")
        self.save_btn.add_css_class("flat")
        self.save_btn.connect("clicked", self._on_save_profile)
        profile_bar.append(self.save_btn)

        # ── Dashboard (status on top) ──
        content.append(SectionHeader("dialog-information", "系统状态"))

        # GPU dashboard card
        self.gpu_card = DashboardCard("gpu", "GPU")
        self.gpu_card.add_row("power", "功耗")
        self.gpu_card.add_row("temp", "温度")
        self.gpu_card.add_row("util", "利用率")
        self.gpu_card.add_row("mem", "显存")
        self.gpu_card.add_row("ec_power", "EC 功率")
        content.append(self.gpu_card)

        # CPU dashboard card
        self.cpu_card = DashboardCard("cpu", "CPU")
        self.cpu_card.add_row("freq", "频率")
        self.cpu_card.add_row("temp", "温度")
        self.cpu_card.add_row("gov", "调频策略")
        self.cpu_card.add_row("epp", "能效偏好")
        content.append(self.cpu_card)

        # System dashboard card
        self.sys_card = DashboardCard("computer", "系统")
        self.sys_card.add_row("fan", "风扇")
        self.sys_card.add_row("battery", "电池")
        self.sys_card.add_row("charge", "充电模式")
        self.sys_card.add_row("ac", "电源")
        self.sys_card.add_row("tlp", "TLP")
        self.sys_card.add_row("nv_powerd", "nvidia-powerd")
        content.append(self.sys_card)

        # ── GPU section ──
        content.append(SectionHeader("gpu", "GPU 功率控制"))

        gpu_attrs = [
            ("gpu_nv_ctgp", "GPU cTGP", "W", "可配置总图形功率，影响 GPU 最大功耗上限"),
            ("gpu_nv_ppab", "GPU PPAB", "W", "功率加速，GPU 高负载时临时额外功率"),
            ("gpu_nv_ac_offset", "GPU AC Offset", "W", "电源适配器接入时的功率偏移"),
            ("gpu_nv_cpu_boost", "GPU→CPU Boost", "W", "GPU 活跃时动态分配给 CPU 的额外功率"),
            ("gpu_temp", "GPU 温度限制", "°C", "GPU 降频温度墙，达到后开始降频"),
        ]
        for attr_name, title, unit, desc in gpu_attrs:
            card = PowerCard(title, unit, description=desc)
            self.cards[attr_name] = card
            content.append(card)

        # ── CPU section ──
        content.append(SectionHeader("cpu", "CPU 功率控制"))

        cpu_ec_attrs = [
            ("ppt_pl1_spl", "CPU PL1", "W", "持续功率限制，长时间负载的最大功耗"),
            ("ppt_pl2_sppt", "CPU PL2", "W", "短时功率限制，Turbo Boost 时的最大功耗"),
            ("ppt_pl3_fppt", "CPU PL3", "W", "快速功率跟踪，最短时间内的峰值功率"),
            ("ppt_cpu_cl", "CPU 交叉负载", "W", "GPU 活跃时的 CPU 功率限制"),
            ("cpu_temp", "CPU 温度限制", "°C", "CPU 降频温度墙，达到后开始降频"),
        ]
        for attr_name, title, unit, desc in cpu_ec_attrs:
            card = PowerCard(title, unit, description=desc)
            self.cards[attr_name] = card
            content.append(card)

        # ── CPU frequency section ──
        content.append(SectionHeader("power-system", "CPU 频率控制"))

        self.freq_card = PowerCard(
            "CPU 最大频率", "MHz", 400, 5000,
            description="CPU 允许达到的最大频率，降低可减少发热和风扇噪音"
        )
        content.append(self.freq_card)

        # ── Advanced options (collapsed) ──
        content.append(SectionHeader("emblem-system", "高级选项"))

        # Collapsible advanced section
        self._advanced_expanded = False
        self._advanced_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self._advanced_box.set_margin_start(12)
        self._advanced_box.set_margin_end(12)
        self._advanced_box.set_margin_bottom(8)
        content.append(self._advanced_box)

        # Toggle button for advanced options
        adv_toggle = Gtk.Button(label="▶ 展开高级选项")
        adv_toggle.add_css_class("flat")
        adv_toggle.set_halign(Gtk.Align.START)
        adv_toggle.connect("clicked", self._toggle_advanced)
        self._advanced_toggle = adv_toggle
        self._advanced_box.append(adv_toggle)

        # Advanced content (initially hidden)
        self._advanced_content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self._advanced_content.set_margin_top(8)
        self._advanced_content.set_visible(False)
        self._advanced_box.append(self._advanced_content)

        # Governor
        gov_frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self._advanced_content.append(gov_frame)

        gov_label = Gtk.Label(label="调频策略 (CPU Governor)", xalign=0)
        gov_label.add_css_class("title-4")
        gov_frame.append(gov_label)

        gov_desc = Gtk.Label(
            label="performance: 始终最高频率 | powersave: 省电优先",
            xalign=0, wrap=True
        )
        gov_desc.add_css_class("caption")
        gov_desc.add_css_class("dim-label")
        gov_frame.append(gov_desc)

        self.gov_combo = Gtk.ComboBoxText()
        self.gov_combo.append_text("performance")
        self.gov_combo.append_text("powersave")
        self.gov_combo.set_active(0)
        gov_frame.append(self.gov_combo)

        # EPP
        epp_frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self._advanced_content.append(epp_frame)

        epp_label = Gtk.Label(label="能效偏好 (Energy Performance Preference)", xalign=0)
        epp_label.add_css_class("title-4")
        epp_frame.append(epp_label)

        epp_desc = Gtk.Label(
            label="performance: 性能优先 | balance_performance: 偏性能 | balance_power: 偏省电 | power: 省电优先",
            xalign=0, wrap=True
        )
        epp_desc.add_css_class("caption")
        epp_desc.add_css_class("dim-label")
        epp_frame.append(epp_desc)

        self.epp_combo = Gtk.ComboBoxText()
        self.epp_combo.append_text("performance")
        self.epp_combo.append_text("balance_performance")
        self.epp_combo.append_text("balance_power")
        self.epp_combo.append_text("power")
        self.epp_combo.set_active(0)
        epp_frame.append(self.epp_combo)

        # Platform profile
        pp_frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self._advanced_content.append(pp_frame)

        pp_label = Gtk.Label(label="平台配置 (Platform Profile)", xalign=0)
        pp_label.add_css_class("title-4")
        pp_frame.append(pp_label)

        pp_desc = Gtk.Label(
            label="控制 EC 整体热策略。应用设置时会自动切换到 custom 模式",
            xalign=0, wrap=True
        )
        pp_desc.add_css_class("caption")
        pp_desc.add_css_class("dim-label")
        pp_frame.append(pp_desc)

        self.pp_combo = Gtk.ComboBoxText()
        for choice in ("low-power", "balanced", "performance", "max-power"):
            self.pp_combo.append_text(choice)
        self.pp_combo.set_active(1)
        pp_frame.append(self.pp_combo)

        # Bottom padding
        bottom_pad = Gtk.Box()
        bottom_pad.set_margin_bottom(24)
        content.append(bottom_pad)

        self.refresh()
        self.start_auto_refresh()

    def _toggle_advanced(self, btn):
        self._advanced_expanded = not self._advanced_expanded
        self._advanced_content.set_visible(self._advanced_expanded)
        self._advanced_toggle.set_label(
            "▼ 收起高级选项" if self._advanced_expanded else "▶ 展开高级选项"
        )

    def start_auto_refresh(self):
        """Start auto-refreshing dashboard data every 3 seconds."""
        if self._auto_refresh_id is None:
            self._auto_refresh_id = GLib.timeout_add_seconds(3, self._auto_refresh_tick)

    def stop_auto_refresh(self):
        """Stop auto-refresh."""
        if self._auto_refresh_id is not None:
            GLib.source_remove(self._auto_refresh_id)
            self._auto_refresh_id = None

    def _auto_refresh_tick(self):
        """Auto-refresh callback - only updates dashboard, not sliders."""
        if not self._loading:
            self._refresh_dashboard()
        return True  # Continue polling

    def _refresh_dashboard(self):
        """Refresh only the dashboard status cards (fast, no sliders)."""
        d = ec_backend.read_dashboard()

        # Thermal banner
        mode = d["mode"]
        custom_active = (mode == "custom")
        # Determine the Fn+Q mode (what LED shows)
        fnq_mode = mode if mode in ("low-power", "balanced", "performance", "max-power") else "balanced"
        self.thermal_banner.set_mode(fnq_mode, custom_active)

        # GPU card
        gpu = d["gpu"]
        if gpu:
            self.gpu_card.set_value("power", f"{gpu['power_draw']:.1f} W")
            self.gpu_card.set_value("temp", f"{gpu['temp']}°C")
            self.gpu_card.set_value("util", f"{gpu['gpu_util']}%")
            self.gpu_card.set_value("mem", f"{gpu['mem_used']} / {gpu['mem_total']} MiB")
        else:
            self.gpu_card.set_value("power", "N/A")
            self.gpu_card.set_value("temp", "N/A")
            self.gpu_card.set_value("util", "N/A")
            self.gpu_card.set_value("mem", "N/A")

        # EC GPU power summary
        ec = d["ec"]
        ctgp = ec.get("gpu_nv_ctgp", {}).get("value")
        ppab = ec.get("gpu_nv_ppab", {}).get("value")
        if ctgp is not None and ppab is not None:
            self.gpu_card.set_value("ec_power", f"cTGP {ctgp}W + PPAB {ppab}W")
        else:
            self.gpu_card.set_value("ec_power", "N/A")

        # CPU card
        cpu = d["cpu"]
        cpu_freq = cpu.get("scaling_max_freq")
        cpu_max = cpu.get("cpuinfo_max_freq")
        if cpu_freq and cpu_max:
            self.cpu_card.set_value("freq", f"{int(cpu_freq) // 1000} / {int(cpu_max) // 1000} MHz")
        else:
            self.cpu_card.set_value("freq", "N/A")

        self.cpu_card.set_value("temp", f"{d['cpu_temp']}°C" if d["cpu_temp"] else "N/A")
        self.cpu_card.set_value("gov", cpu.get("scaling_governor", "N/A"))
        self.cpu_card.set_value("epp", cpu.get("energy_performance_preference", "N/A"))

        # System card
        fan = d["fan"]
        if fan:
            fan_strs = [f"{v} RPM" for v in fan.values()]
            self.sys_card.set_value("fan", " | ".join(fan_strs))
        else:
            self.sys_card.set_value("fan", "N/A")

        bat = d["battery"]
        if bat.get("capacity") is not None:
            bat_str = f"{bat['capacity']}%"
            if bat.get("status"):
                bat_str += f" ({bat['status']})"
            self.sys_card.set_value("battery", bat_str)
        else:
            self.sys_card.set_value("battery", "N/A")

        # Battery charge type
        charge = d.get("charge_type")
        charge_map = {
            "Fast": "快速充电",
            "Standard": "标准充电",
            "Long_Life": "长寿模式 (80%)",
        }
        self.sys_card.set_value("charge", charge_map.get(charge, charge or "N/A"))

        ac = d["ac_power"]
        self.sys_card.set_value("ac", "AC 通电" if ac else "电池供电" if ac is not None else "N/A")

        tlp = d["tlp"]
        self.sys_card.set_value("tlp", "active" if tlp else "inactive" if tlp is not None else "N/A")

        nv = d["nv_powerd"]
        self.sys_card.set_value("nv_powerd", "active" if nv else "inactive" if nv is not None else "N/A")

    def refresh(self):
        """Reload all values from hardware."""
        self._loading = True

        # Refresh dashboard
        self._refresh_dashboard()

        # Read EC for sliders
        self.ec_data = ec_backend.read_all_ec()
        for attr_name, data in self.ec_data.items():
            if attr_name in self.cards:
                self.cards[attr_name].set_value(data["value"])

        # Read CPU for sliders
        self.cpu_data = ec_backend.read_cpu_freq()
        max_freq_mhz = None
        if "scaling_max_freq" in self.cpu_data:
            try:
                max_freq_mhz = int(self.cpu_data["scaling_max_freq"]) // 1000
            except (ValueError, TypeError):
                pass
        if max_freq_mhz is not None:
            self.freq_card.set_value(max_freq_mhz)

        gov = self.cpu_data.get("scaling_governor", "performance")
        self.gov_combo.set_active(0 if gov == "performance" else 1)

        epp = self.cpu_data.get("energy_performance_preference", "performance")
        epp_map = {"performance": 0, "balance_performance": 1, "balance_power": 2, "power": 3}
        self.epp_combo.set_active(epp_map.get(epp, 0))

        pp = ec_backend.read_platform_profile()
        if pp:
            pp_choices = ec_backend.read_platform_profile_choices()
            for i, c in enumerate(pp_choices):
                if c == pp:
                    self.pp_combo.set_active(i)
                    break

        self._loading = False

    def apply_all(self):
        """Apply all current slider values to hardware."""
        ec_values = {}
        for attr_name, card in self.cards.items():
            if attr_name in ec_backend.EC_ATTRIBUTES:
                ec_values[attr_name] = card.get_value()

        cpu_freq_mhz = self.freq_card.get_value()
        governor = self.gov_combo.get_active_text()
        epp = self.epp_combo.get_active_text()
        pp = self.pp_combo.get_active_text()

        ok, err = ec_backend.apply_all_settings(
            ec_values=ec_values,
            cpu_freq_mhz=cpu_freq_mhz,
            governor=governor,
            epp=epp,
            platform_profile=pp,
        )

        GLib.timeout_add(500, self.refresh)

        if not ok:
            return [err]
        return []

    def collect_current_settings(self):
        settings = {}
        for attr_name, card in self.cards.items():
            settings[attr_name] = card.get_value()
        settings["cpu_scaling_max_freq"] = self.freq_card.get_value() * 1000
        settings["cpu_governor"] = self.gov_combo.get_active_text()
        settings["cpu_epp"] = self.epp_combo.get_active_text()
        settings["platform_profile"] = self.pp_combo.get_active_text()
        return settings

    def apply_profile_settings(self, settings):
        self._loading = True
        for attr_name, card in self.cards.items():
            if attr_name in settings:
                card.set_value(settings[attr_name])

        if "cpu_scaling_max_freq" in settings:
            self.freq_card.set_value(settings["cpu_scaling_max_freq"] // 1000)

        gov = settings.get("cpu_governor", "performance")
        self.gov_combo.set_active(0 if gov == "performance" else 1)

        epp = settings.get("cpu_epp", "performance")
        epp_map = {"performance": 0, "balance_performance": 1, "balance_power": 2, "power": 3}
        self.epp_combo.set_active(epp_map.get(epp, 0))

        pp = settings.get("platform_profile", "balanced")
        pp_choices = ec_backend.read_platform_profile_choices()
        for i, c in enumerate(pp_choices):
            if c == pp:
                self.pp_combo.set_active(i)
                break

        self._loading = False

    def set_profiles(self, profile_names, active_name=None, on_select=None):
        while child := self.profile_box.get_first_child():
            self.profile_box.remove(child)
        self.profile_buttons.clear()

        for name in profile_names:
            btn = ProfileButton(name)
            if name == active_name:
                btn.set_active(True)
            btn.connect("toggled", lambda b, n=name: on_select(n) if b.get_active() and on_select else None)
            self.profile_buttons[name] = btn
            self.profile_box.append(btn)

    def _on_save_profile(self, btn):
        win = self.get_root()
        if win and hasattr(win, "_on_save_profile"):
            win._on_save_profile()
