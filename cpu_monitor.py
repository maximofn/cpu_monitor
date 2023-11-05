#!/usr/bin/env python3
import signal
import gi
gi.require_version('AppIndicator3', '0.1')
from gi.repository import AppIndicator3, GLib
from gi.repository import Gtk as gtk
import os
import subprocess

APPINDICATOR_ID = 'GPU_monitor'

def main():
    path = os.path.dirname(os.path.realpath(__file__))
    icon_path = os.path.abspath(f"{path}/cpu.png")
    CPU_indicator = AppIndicator3.Indicator.new(APPINDICATOR_ID, icon_path, AppIndicator3.IndicatorCategory.SYSTEM_SERVICES)
    CPU_indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
    CPU_indicator.set_menu(build_menu())

    # Get CPU info
    GLib.timeout_add_seconds(1, update_cpu_info, CPU_indicator)

    GLib.MainLoop().run()

def build_menu():
    menu = gtk.Menu()
    item_hello = gtk.MenuItem(label='Hola Mundo')
    # item_hello.connect('activate', hello)
    menu.append(item_hello)
    # menu.show_all()
    return menu

def update_cpu_info(indicator):
    cpu_temps = get_cpu_info()

    info = f"{cpu_temps['Tctl']}ºC"

    indicator.set_label(info, "Indicator")

    return True

def get_cpu_info():
    sensors_output = subprocess.check_output(['sensors']).decode('utf-8')
    temperatures = {}
    for line in sensors_output.split("\n"):
        if "Tctl" in line:
            # Asumiendo que el formato es "Tctl:         +XX.X°C"
            temp = float(line.split('+')[1].split('°')[0])
            temperatures['Tctl'] = temp
        if "Tccd1" in line:
            # Asumiendo que el formato es "Tccd1:        +XX.X°C"
            temp = float(line.split('+')[1].split('°')[0])
            temperatures['Tccd1'] = temp
    return temperatures

if __name__ == "__main__":
    signal.signal(signal.SIGINT, signal.SIG_DFL) # Allow the program to be terminated with Ctrl+C
    main()
