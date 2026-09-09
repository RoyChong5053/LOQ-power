"""Custom GTK4 widgets for power control UI."""

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gdk, GLib


class PowerCard(Gtk.Box):
    """A card widget for a single power parameter with label, value, and slider."""

    def __init__(self, title, unit="W", min_val=0, max_val=100,
                 current=None, default=None, description=None):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.unit = unit
        self.min_val = min_val
        self.max_val = max_val
        self.default_val = default
        self._changed_callback = None

        self.add_css_class("power-card")
        self.set_margin_start(12)
        self.set_margin_end(12)
        self.set_margin_top(6)
        self.set_margin_bottom(6)

        # Header row: title + value label
        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.append(header)

        title_label = Gtk.Label(label=title, xalign=0)
        title_label.add_css_class("title-4")
        header.append(title_label)

        spacer = Gtk.Box()
        header.append(spacer)
        spacer.set_hexpand(True)

        self.value_label = Gtk.Label(label="--", xalign=1)
        self.value_label.add_css_class("numeric")
        self.value_label.add_css_class("dim-label")
        header.append(self.value_label)

        # Description row
        if description:
            desc_label = Gtk.Label(label=description, xalign=0)
            desc_label.add_css_class("caption")
            desc_label.add_css_class("dim-label")
            desc_label.add_css_class("slider-desc")
            self.append(desc_label)

        # Slider
        self.slider = Gtk.Scale.new_with_range(
            Gtk.Orientation.HORIZONTAL, min_val, max_val, 1
        )
        self.slider.set_draw_value(False)
        self.slider.set_hexpand(True)
        self.slider.set_size_request(200, -1)
        self.slider.set_focus_on_click(False)
        self.slider.connect("value-changed", self._on_slider_changed)

        # Allow scroll events to pass through to ScrolledWindow
        scroll_ctrl = Gtk.EventControllerScroll.new(
            Gtk.EventControllerScrollFlags.VERTICAL
        )
        scroll_ctrl.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        scroll_ctrl.connect("scroll", self._on_slider_scroll)
        self.slider.add_controller(scroll_ctrl)

        self.append(self.slider)

        # Footer row: range info
        footer = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        self.append(footer)

        min_label = Gtk.Label(label=f"{min_val}{unit}", xalign=0)
        min_label.add_css_class("caption")
        min_label.add_css_class("dim-label")
        footer.append(min_label)

        footer.append(Gtk.Box())  # spacer

        if default is not None:
            def_label = Gtk.Label(label=f"默认 {default}{unit}", xalign=0.5)
            def_label.add_css_class("caption")
            def_label.add_css_class("accent")
            footer.append(def_label)
            footer.append(Gtk.Box())  # spacer

        max_label = Gtk.Label(label=f"{max_val}{unit}", xalign=1)
        max_label.add_css_class("caption")
        max_label.add_css_class("dim-label")
        footer.append(max_label)

        if current is not None:
            self.set_value(current)

    def set_value(self, val):
        if val is not None:
            self.slider.set_value(val)
            self.value_label.set_text(f"{val}{self.unit}")

    def get_value(self):
        return int(self.slider.get_value())

    def set_changed_callback(self, cb):
        self._changed_callback = cb

    def _on_slider_changed(self, scale):
        val = int(scale.get_value())
        self.value_label.set_text(f"{val}{self.unit}")
        if self._changed_callback:
            self._changed_callback(val)

    def _on_slider_scroll(self, controller, x, y):
        return False


class SectionHeader(Gtk.Box):
    """A section header with an icon and title."""

    def __init__(self, icon_name, title):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.set_margin_top(16)
        self.set_margin_bottom(4)
        self.set_margin_start(12)
        self.set_margin_end(12)

        icon = Gtk.Image.new_from_icon_name(icon_name)
        icon.set_pixel_size(20)
        self.append(icon)

        label = Gtk.Label(label=title, xalign=0)
        label.add_css_class("title-3")
        self.append(label)


class StatusRow(Gtk.Box):
    """A single status info row with label and value."""

    def __init__(self, label_text, value_text=""):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.set_margin_start(12)
        self.set_margin_end(12)
        self.set_margin_top(2)
        self.set_margin_bottom(2)

        label = Gtk.Label(label=label_text, xalign=0)
        label.set_hexpand(True)
        label.add_css_class("dim-label")
        self.append(label)

        self.value_label = Gtk.Label(label=value_text, xalign=1)
        self.value_label.add_css_class("numeric")
        self.append(self.value_label)

    def set_value(self, text):
        self.value_label.set_text(str(text))


class ProfileButton(Gtk.ToggleButton):
    """A toggle button for profile selection."""

    def __init__(self, profile_name):
        super().__init__(label=profile_name)
        self.profile_name = profile_name
        self.add_css_class("suggested-action")


class DashboardRow(Gtk.Box):
    """A single row in the dashboard: label + value, compact layout."""

    def __init__(self, label_text, value_text="--"):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.set_margin_top(2)
        self.set_margin_bottom(2)
        self.set_margin_start(4)
        self.set_margin_end(4)

        self._label = Gtk.Label(label=label_text, xalign=0)
        self._label.set_hexpand(True)
        self._label.add_css_class("caption")
        self._label.add_css_class("dim-label")
        self.append(self._label)

        self._value = Gtk.Label(label=value_text, xalign=1)
        self._value.add_css_class("caption")
        self._value.add_css_class("numeric")
        self.append(self._value)

    def set_value(self, text):
        self._value.set_text(str(text))

    def set_value_css(self, css_class):
        self._value.add_css_class(css_class)


class DashboardCard(Gtk.Box):
    """A card containing a section title and multiple DashboardRows."""

    def __init__(self, icon_name, title):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        self.set_margin_start(12)
        self.set_margin_end(12)
        self.set_margin_top(6)
        self.set_margin_bottom(2)
        self.add_css_class("dashboard-card")

        # Section header
        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        header.set_margin_start(8)
        header.set_margin_top(4)
        header.set_margin_bottom(2)
        self.append(header)

        icon = Gtk.Image.new_from_icon_name(icon_name)
        icon.set_pixel_size(16)
        header.append(icon)

        label = Gtk.Label(label=title, xalign=0)
        label.add_css_class("title-4")
        header.append(label)

        # Rows container with border
        self._rows_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self._rows_box.set_margin_start(24)
        self._rows_box.set_margin_end(8)
        self._rows_box.set_margin_bottom(4)
        self.append(self._rows_box)

        self._rows = {}

    def add_row(self, key, label, value="--"):
        row = DashboardRow(label, value)
        self._rows[key] = row
        self._rows_box.append(row)
        return row

    def set_value(self, key, text):
        if key in self._rows:
            self._rows[key].set_value(text)


class ThermalBanner(Gtk.Box):
    """A colored banner showing the current thermal mode with LED color."""

    MODE_STYLES = {
        "low-power":  {"color": "#3584e4", "css": "mode-quiet",    "label": "省电", "led": "蓝灯"},
        "balanced":   {"color": "#ffffff", "css": "mode-balanced", "label": "均衡", "led": "白灯"},
        "performance":{"color": "#e01b24", "css": "mode-perf",     "label": "性能", "led": "红灯"},
        "max-power":  {"color": "#ff7800", "css": "mode-extreme",  "label": "极速", "led": "紫灯"},
        "custom":     {"color": "#9141ac", "css": "mode-custom",   "label": "自定义", "led": "紫灯"},
    }

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.set_margin_start(12)
        self.set_margin_end(12)
        self.set_margin_top(10)
        self.set_margin_bottom(6)
        self.add_css_class("osd")
        self.add_css_class("thermal-banner")

        # LED color indicator dot
        self._dot = Gtk.DrawingArea()
        self._dot.set_size_request(12, 12)
        self._dot.set_halign(Gtk.Align.CENTER)
        self._dot.set_valign(Gtk.Align.CENTER)
        self.append(self._dot)

        self._label = Gtk.Label(label="检测中...", xalign=0, wrap=True)
        self._label.set_hexpand(True)
        self.append(self._label)

        self._current_mode = None

    def set_mode(self, mode, custom_active=False):
        """Update banner for the given mode.

        Args:
            mode: The Fn+Q mode (low-power/balanced/performance/max-power)
            custom_active: Whether CUSTOM mode is active for EC writes
        """
        self._current_mode = mode
        info = self.MODE_STYLES.get(mode, self.MODE_STYLES["balanced"])

        # Remove old mode CSS classes
        for m_info in self.MODE_STYLES.values():
            self.remove_css_class(m_info["css"])

        # Set new mode CSS class
        self.add_css_class(info["css"])

        # Update dot color via CSS
        self._dot.remove_css_class("dot-quiet")
        self._dot.remove_css_class("dot-balanced")
        self._dot.remove_css_class("dot-perf")
        self._dot.remove_css_class("dot-extreme")
        self._dot.remove_css_class("dot-custom")
        self._dot.add_css_class(f"dot-{info['css'].replace('mode-', '')}")

        # Build label text
        if custom_active:
            self._label.set_text(
                f"{info['led']} {info['label']}模式  ✓ EC 功率已激活"
            )
        else:
            self._label.set_text(
                f"{info['led']} {info['label']}模式"
            )
