#!/usr/bin/env python3
"""Small AppIndicator used only as deterministic guest evidence."""

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("AppIndicator3", "0.1")
from gi.repository import AppIndicator3, Gtk  # noqa: E402

indicator = AppIndicator3.Indicator.new(
    "sniptale-e2e-probe",
    "dialog-information",
    AppIndicator3.IndicatorCategory.APPLICATION_STATUS,
)
indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
indicator.set_title("Sniptale Lab Probe")
menu = Gtk.Menu()
item = Gtk.MenuItem(label="Sniptale Lab Probe")
item.show()
menu.append(item)
indicator.set_menu(menu)
Gtk.main()
