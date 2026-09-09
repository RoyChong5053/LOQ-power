"""Main GTK4 application window."""

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gio, GLib
from gui.power_page import PowerPage
import profiles as prof


class LOQPowerApp(Gtk.Application):
    """Main application."""

    def __init__(self):
        super().__init__(
            application_id="com.github.loq-power",
            flags=Gio.ApplicationFlags.FLAGS_NONE,
        )
        self.window = None

    def do_activate(self):
        if self.window is None:
            self.window = LOQPowerWindow(application=self)
        self.window.present()


class LOQPowerWindow(Gtk.ApplicationWindow):
    """Main window."""

    def __init__(self, **kwargs):
        super().__init__(
            title="LOQ Power Control",
            default_width=520,
            default_height=720,
            **kwargs,
        )

        self.profiles_data = prof.load_profiles()
        self.active_profile_name = None

        # Main layout
        main_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.set_child(main_box)

        # Header bar
        header = Gtk.HeaderBar()
        self.set_titlebar(header)

        # Refresh button
        refresh_btn = Gtk.Button()
        refresh_icon = Gtk.Image.new_from_icon_name("view-refresh-symbolic")
        refresh_btn.set_child(refresh_icon)
        refresh_btn.set_tooltip_text("刷新硬件状态")
        refresh_btn.connect("clicked", lambda b: self._refresh())
        header.pack_start(refresh_btn)

        # Apply button
        self.apply_btn = Gtk.Button(label="应用")
        self.apply_btn.add_css_class("suggested-action")
        self.apply_btn.set_tooltip_text("将当前设置写入硬件")
        self.apply_btn.connect("clicked", lambda b: self._apply())
        header.pack_end(self.apply_btn)

        # Power page
        self.page = PowerPage()
        main_box.append(self.page)

        # Status bar
        self.statusbar = Gtk.Label(label="就绪", xalign=0)
        self.statusbar.add_css_class("caption")
        self.statusbar.add_css_class("dim-label")
        self.statusbar.set_margin_start(12)
        self.statusbar.set_margin_end(12)
        self.statusbar.set_margin_top(4)
        self.statusbar.set_margin_bottom(4)
        main_box.append(self.statusbar)

        # Load profiles into the UI
        self._refresh_profile_bar()

        # Initial data load
        self._refresh()

    def _refresh_profile_bar(self):
        names = list(self.profiles_data.keys())
        self.page.set_profiles(
            names,
            active_name=self.active_profile_name,
            on_select=self._on_profile_select,
        )

    def _on_profile_select(self, name):
        if name in self.profiles_data:
            self.active_profile_name = name
            self.page.apply_profile_settings(self.profiles_data[name])
            self.statusbar.set_text(f"已加载方案: {name}")

    def _refresh(self):
        self.statusbar.set_text("正在读取硬件状态...")
        self.page.refresh()
        self.statusbar.set_text("硬件状态已刷新")

    def _apply(self):
        self.statusbar.set_text("正在写入设置...")
        errors = self.page.apply_all()
        if errors:
            self.statusbar.set_text(f"部分写入失败: {'; '.join(errors[:3])}")
        else:
            self.statusbar.set_text("设置已应用")

    def _on_save_profile(self):
        """Show a dialog to save the current settings as a profile."""
        dialog = Gtk.Window(
            title="保存方案",
            transient_for=self,
            modal=True,
            default_width=350,
            default_height=150,
        )

        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        vbox.set_margin_top(18)
        vbox.set_margin_bottom(18)
        vbox.set_margin_start(18)
        vbox.set_margin_end(18)
        dialog.set_child(vbox)

        label = Gtk.Label(label="输入方案名称:", xalign=0)
        vbox.append(label)

        entry = Gtk.Entry()
        entry.set_hexpand(True)
        if self.active_profile_name:
            entry.set_text(self.active_profile_name)
        vbox.append(entry)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        btn_box.set_halign(Gtk.Align.END)
        vbox.append(btn_box)

        cancel_btn = Gtk.Button(label="取消")
        cancel_btn.connect("clicked", lambda b: dialog.close())
        btn_box.append(cancel_btn)

        save_btn = Gtk.Button(label="保存")
        save_btn.add_css_class("suggested-action")
        btn_box.append(save_btn)

        def on_save(b):
            name = entry.get_text().strip()
            if not name:
                return
            settings = self.page.collect_current_settings()
            prof.save_user_profile(name, settings)
            self.profiles_data = prof.load_profiles()
            self._refresh_profile_bar()
            self.active_profile_name = name
            self.page.set_profiles(
                list(self.profiles_data.keys()),
                active_name=self.active_profile_name,
                on_select=self._on_profile_select,
            )
            self.statusbar.set_text(f"方案已保存: {name}")
            dialog.close()

        save_btn.connect("clicked", on_save)
        entry.connect("activate", lambda e: on_save(None))

        dialog.present()
