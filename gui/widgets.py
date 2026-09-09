"""Custom GTK4 widgets for power control UI."""

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gdk, GLib


class PowerCard(Gtk.Box):
    """A card widget for a single power parameter with label, value, and slider."""

    def __init__(self, title, unit="W", min_val=0, max_val=100, current=None, default=None):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.unit = unit
        self.min_val = min_val
        self.max_val = max_val
        self.default_val = default
        self._changed_callback = None

        self.add_css_class("power-card")
        self.set_margin_start(12)
        self.set_margin_end(12)
        self.set_margin_top(8)
        self.set_margin_bottom(8)

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

        # Slider
        self.slider = Gtk.Scale.new_with_range(
            Gtk.Orientation.HORIZONTAL, min_val, max_val, 1
        )
        self.slider.set_draw_value(False)
        self.slider.set_hexpand(True)
        self.slider.set_size_request(200, -1)
        self.slider.connect("value-changed", self._on_slider_changed)
        self.append(self.slider)

        # Footer row: min, default, max
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
