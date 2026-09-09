"""Power control page with GPU/CPU sliders and status info."""

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib
from gui.widgets import PowerCard, SectionHeader, StatusRow, ProfileButton
import ec_backend
import profiles as prof


class PowerPage(Gtk.Box):
    """Main power control page."""

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.set_vexpand(True)

        # State
        self.ec_data = {}
        self.cpu_data = {}
        self.nvidia_data = {}
        self.cards = {}
        self.profile_buttons = {}
        self._active_profile = None
        self._loading = False

        # Scrollable content
        scroll = Gtk.ScrolledWindow()
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.append(scroll)

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        scroll.set_child(content)

        # ── Profile bar ──
        profile_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        profile_bar.set_margin_start(12)
        profile_bar.set_margin_end(12)
        profile_bar.set_margin_top(12)
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

        # ── GPU section ──
        content.append(SectionHeader("gpu", "GPU 功率控制"))

        gpu_attrs = [
            ("gpu_nv_ctgp", "GPU cTGP (可配置总功率)", "W"),
            ("gpu_nv_ppab", "GPU PPAB (功率加速)", "W"),
            ("gpu_nv_ac_offset", "GPU AC Offset (总功率偏移)", "W"),
            ("gpu_nv_cpu_boost", "GPU→CPU Dynamic Boost", "W"),
            ("gpu_temp", "GPU 温度限制", "°C"),
        ]
        for attr_name, title, unit in gpu_attrs:
            card = PowerCard(title, unit)
            self.cards[attr_name] = card
            content.append(card)

        # ── CPU section ──
        content.append(SectionHeader("cpu", "CPU 功率控制"))

        cpu_ec_attrs = [
            ("ppt_pl1_spl", "CPU PL1 (持续功率)", "W"),
            ("ppt_pl2_sppt", "CPU PL2 (短时功率)", "W"),
            ("ppt_pl3_fppt", "CPU PL3 (快速功率)", "W"),
            ("ppt_cpu_cl", "CPU 交叉负载功率", "W"),
            ("cpu_temp", "CPU 温度限制", "°C"),
        ]
        for attr_name, title, unit in cpu_ec_attrs:
            card = PowerCard(title, unit)
            self.cards[attr_name] = card
            content.append(card)

        # ── CPU frequency section ──
        content.append(SectionHeader("power-system", "CPU 频率控制"))

        self.freq_card = PowerCard("CPU 最大频率", "MHz", 400, 5000)
        content.append(self.freq_card)

        # Governor row
        gov_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        gov_box.set_margin_start(12)
        gov_box.set_margin_end(12)
        gov_box.set_margin_top(8)
        content.append(gov_box)

        gov_label = Gtk.Label(label="调频策略:", xalign=0)
        gov_label.add_css_class("dim-label")
        gov_box.append(gov_label)

        self.gov_combo = Gtk.ComboBoxText()
        self.gov_combo.append_text("performance")
        self.gov_combo.append_text("powersave")
        self.gov_combo.set_active(0)
        gov_box.append(self.gov_combo)

        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        gov_box.append(spacer)

        epp_label = Gtk.Label(label="能效偏好:", xalign=0)
        epp_label.add_css_class("dim-label")
        gov_box.append(epp_label)

        self.epp_combo = Gtk.ComboBoxText()
        self.epp_combo.append_text("performance")
        self.epp_combo.append_text("balance_performance")
        self.epp_combo.append_text("balance_power")
        self.epp_combo.append_text("power")
        self.epp_combo.set_active(0)
        gov_box.append(self.epp_combo)

        # Platform profile row
        pp_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        pp_box.set_margin_start(12)
        pp_box.set_margin_end(12)
        pp_box.set_margin_top(4)
        pp_box.set_margin_bottom(8)
        content.append(pp_box)

        pp_label = Gtk.Label(label="平台配置:", xalign=0)
        pp_label.add_css_class("dim-label")
        pp_box.append(pp_label)

        self.pp_combo = Gtk.ComboBoxText()
        for choice in ("low-power", "balanced", "performance", "max-power"):
            self.pp_combo.append_text(choice)
        self.pp_combo.set_active(1)
        pp_box.append(self.pp_combo)

        # ── Status section ──
        content.append(SectionHeader("dialog-information", "当前状态"))

        self.status_gpu_power = StatusRow("GPU 功率 (nvidia-smi)")
        content.append(self.status_gpu_power)

        self.status_gpu_limit = StatusRow("GPU 限制")
        content.append(self.status_gpu_limit)

        self.status_ec_state = StatusRow("EC 状态")
        content.append(self.status_ec_state)

        self.status_cpu_freq = StatusRow("CPU 频率")
        content.append(self.status_cpu_freq)

        # Bottom padding
        bottom_pad = Gtk.Box()
        bottom_pad.set_margin_bottom(24)
        content.append(bottom_pad)

        # Load data
        self.refresh()

    def refresh(self):
        """Reload all values from hardware."""
        self._loading = True

        # Read EC
        self.ec_data = ec_backend.read_all_ec()
        for attr_name, data in self.ec_data.items():
            if attr_name in self.cards:
                card = self.cards[attr_name]
                card.set_value(data["value"])

        # Read CPU
        self.cpu_data = ec_backend.read_cpu_freq()
        max_freq_mhz = None
        if "scaling_max_freq" in self.cpu_data:
            try:
                max_freq_mhz = int(self.cpu_data["scaling_max_freq"]) // 1000
            except (ValueError, TypeError):
                pass
        if max_freq_mhz is not None:
            self.freq_card.set_value(max_freq_mhz)

        # Governor
        gov = self.cpu_data.get("scaling_governor", "performance")
        idx = 0 if gov == "performance" else 1
        self.gov_combo.set_active(idx)

        # EPP
        epp = self.cpu_data.get("energy_performance_preference", "performance")
        epp_map = {
            "performance": 0,
            "balance_performance": 1,
            "balance_power": 2,
            "power": 3,
        }
        self.epp_combo.set_active(epp_map.get(epp, 0))

        # Platform profile
        pp = ec_backend.read_platform_profile()
        if pp:
            pp_choices = ec_backend.read_platform_profile_choices()
            for i, c in enumerate(pp_choices):
                if c == pp:
                    self.pp_combo.set_active(i)
                    break

        # NVIDIA
        self.nvidia_data = ec_backend.read_nvidia_power() or {}
        gpu_power = self.nvidia_data.get("power_draw")
        gpu_limit = self.nvidia_data.get("current_power_limit")
        gpu_max = self.nvidia_data.get("max_power_limit")

        if gpu_power is not None:
            self.status_gpu_power.set_value(f"{gpu_power:.1f} W")
        else:
            self.status_gpu_power.set_value("N/A")

        if gpu_limit is not None:
            limit_str = f"{gpu_limit:.0f} W"
            if gpu_max:
                limit_str += f" / 最大 {gpu_max:.0f} W"
            self.status_gpu_limit.set_value(limit_str)
        else:
            self.status_gpu_limit.set_value("N/A")

        # EC state summary
        ctgp = self.ec_data.get("gpu_nv_ctgp", {}).get("value")
        ppab = self.ec_data.get("gpu_nv_ppab", {}).get("value")
        if ctgp is not None and ppab is not None:
            self.status_ec_state.set_value(f"cTGP={ctgp}W + PPAB={ppab}W = {ctgp + ppab}W")
        else:
            self.status_ec_state.set_value("EC 未就绪")

        # CPU freq summary
        cur_freq = self.cpu_data.get("scaling_max_freq")
        info_max = self.cpu_data.get("cpuinfo_max_freq")
        if cur_freq and info_max:
            self.status_cpu_freq.set_value(
                f"{int(cur_freq) // 1000} MHz / {int(info_max) // 1000} MHz"
            )
        else:
            self.status_cpu_freq.set_value("N/A")

        self._loading = False

    def apply_all(self):
        """Apply all current slider values to hardware."""
        errors = []

        # EC attributes
        for attr_name, card in self.cards.items():
            if attr_name in ec_backend.EC_ATTRIBUTES:
                val = card.get_value()
                ok, err = ec_backend.write_ec_value(attr_name, val)
                if not ok:
                    errors.append(f"{attr_name}: {err}")

        # CPU max freq
        max_freq = self.freq_card.get_value() * 1000
        ok, err = ec_backend.write_cpu_scaling_max_freq(max_freq)
        if not ok:
            errors.append(f"scaling_max_freq: {err}")

        # Governor (all CPUs)
        gov = self.gov_combo.get_active_text()
        if gov:
            ok, err = ec_backend.write_cpu_governor(gov)
            if not ok:
                errors.append(f"governor: {err}")

        # EPP (all CPUs)
        epp = self.epp_combo.get_active_text()
        if epp:
            ok, err = ec_backend.write_cpu_epp(epp)
            if not ok:
                errors.append(f"epp: {err}")

        # Platform profile
        pp = self.pp_combo.get_active_text()
        if pp:
            ok, err = ec_backend.write_platform_profile(pp)
            if not ok:
                errors.append(f"platform_profile: {err}")

        # Refresh to show updated values
        GLib.timeout_add(500, self.refresh)

        return errors

    def collect_current_settings(self):
        """Collect all current slider/combo values into a dict."""
        settings = {}
        for attr_name, card in self.cards.items():
            settings[attr_name] = card.get_value()

        settings["cpu_scaling_max_freq"] = self.freq_card.get_value() * 1000
        settings["cpu_governor"] = self.gov_combo.get_active_text()
        settings["cpu_epp"] = self.epp_combo.get_active_text()
        settings["platform_profile"] = self.pp_combo.get_active_text()
        return settings

    def apply_profile_settings(self, settings):
        """Apply a profile's settings to the UI (does NOT write to hardware)."""
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
        """Build the profile button bar."""
        # Clear existing
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
        """Emit a signal to parent to handle save."""
        # Find the toplevel window and call its save handler
        win = self.get_root()
        if win and hasattr(win, "_on_save_profile"):
            win._on_save_profile()
