#!/usr/bin/env python3
"""LOQ Power Control - EC firmware power management for Lenovo LOQ laptops."""

import sys
import os

# Ensure we can import our modules
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gdk

# Custom CSS for our widgets
CUSTOM_CSS = """
/* Dashboard cards */
.dashboard-card {
    border: 1px solid alpha(@borders, 0.5);
    border-radius: 8px;
    padding: 4px 8px;
    margin: 4px 0;
}

/* Power cards */
.power-card {
    border: 1px solid alpha(@borders, 0.3);
    border-radius: 6px;
    padding: 6px 8px;
    margin: 4px 0;
}

/* Slider description */
.slider-desc {
    opacity: 0.7;
    font-size: 0.85em;
}

/* Thermal banner */
.thermal-banner {
    border-radius: 8px;
    padding: 8px 12px;
}

/* LED color dots */
.dot-quiet {
    background: #3584e4;
    border-radius: 50%;
}
.dot-balanced {
    background: #ffffff;
    border-radius: 50%;
    border: 1px solid alpha(@borders, 0.5);
}
.dot-perf {
    background: #e01b24;
    border-radius: 50%;
}
.dot-extreme {
    background: #9141ac;
    border-radius: 50%;
}
.dot-custom {
    background: #9141ac;
    border-radius: 50%;
}

/* Mode banner colors */
.mode-quiet {
    background: alpha(#3584e4, 0.15);
    border: 1px solid #3584e4;
}
.mode-balanced {
    background: alpha(#ffffff, 0.1);
    border: 1px solid alpha(@borders, 0.5);
}
.mode-perf {
    background: alpha(#e01b24, 0.15);
    border: 1px solid #e01b24;
}
.mode-extreme {
    background: alpha(#9141ac, 0.15);
    border: 1px solid #9141ac;
}
.mode-custom {
    background: alpha(#9141ac, 0.15);
    border: 1px solid #9141ac;
}
"""


def load_css():
    """Load custom CSS styles."""
    css_provider = Gtk.CssProvider()
    css_provider.load_from_string(CUSTOM_CSS)
    Gtk.StyleContext.add_provider_for_display(
        Gdk.Display.get_default(),
        css_provider,
        Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
    )


def main():
    load_css()
    app = LOQPowerApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    sys.exit(main())
