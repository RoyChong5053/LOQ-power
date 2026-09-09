#!/usr/bin/env python3
"""LOQ Power Control - EC firmware power management for Lenovo LOQ laptops."""

import sys
import os

# Ensure we can import our modules
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk
from gui.app import LOQPowerApp


def main():
    app = LOQPowerApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    sys.exit(main())
