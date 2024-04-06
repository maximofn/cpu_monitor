#!/usr/bin/env python3
import signal
import gi
gi.require_version('AppIndicator3', '0.1')
from gi.repository import AppIndicator3, GLib
from gi.repository import Gtk as gtk
import os
import subprocess
import webbrowser

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

def open_repo_link(_):
    webbrowser.open('https://github.com/maximofn/cpu_monitor')

def buy_me_a_coffe(_):
    webbrowser.open('https://www.buymeacoffee.com/maximofn')

def build_menu():
    menu = gtk.Menu()

    cpu_temps = get_cpu_info()

    # info = f"{cpu_temps['Tctl']}ºC"
    if 'Tctl' in cpu_temps.keys():
        cpu_temp = f"{cpu_temps['Tctl']}ºC"
    elif 'Package id 0' in cpu_temps.keys():
        cpu_temp = f"{cpu_temps['Package id 0']}ºC"
    
    cpu_temp_item = gtk.MenuItem(label=f"CPU Temp: {cpu_temp}")
    menu.append(cpu_temp_item)

    horizontal_separator1 = gtk.SeparatorMenuItem()
    menu.append(horizontal_separator1)

    item_repo = gtk.MenuItem(label='Repository')
    item_repo.connect('activate', open_repo_link)
    menu.append(item_repo)

    item_buy_me_a_coffe = gtk.MenuItem(label='Buy me a coffe')
    item_buy_me_a_coffe.connect('activate', buy_me_a_coffe)
    menu.append(item_buy_me_a_coffe)

    horizontal_separator2 = gtk.SeparatorMenuItem()
    menu.append(horizontal_separator2)

    item_quit = gtk.MenuItem(label='Quit')
    item_quit.connect('activate', quit)
    menu.append(item_quit)

    menu.show_all()
    return menu

def update_cpu_info(indicator):
    cpu_temps = get_cpu_info()

    # info = f"{cpu_temps['Tctl']}ºC"
    if 'Tctl' in cpu_temps.keys():
        info = f"{cpu_temps['Tctl']}ºC"
    elif 'Package id 0' in cpu_temps.keys():
        info = f"{cpu_temps['Package id 0']}ºC"

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
        if "Package id 0" in line:
            # Asumiendo que el formato es "Package id 0:  +XX.X°C"
            temp = float(line.split('+')[1].split('°')[0])
            temperatures['Package id 0'] = temp
    return temperatures

if __name__ == "__main__":
    signal.signal(signal.SIGINT, signal.SIG_DFL) # Allow the program to be terminated with Ctrl+C
    main()
