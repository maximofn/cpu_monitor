#!/usr/bin/env python3
import signal
import gi
gi.require_version('AppIndicator3', '0.1')
from gi.repository import AppIndicator3, GLib
from gi.repository import Gtk as gtk
import os
import subprocess
import webbrowser
import matplotlib.pyplot as plt
import time
import re
from PIL import Image, ImageDraw, ImageFont

APPINDICATOR_ID = 'GPU_monitor'

BLUE_COLOR = '#66b3ff'
RED_COLOR = '#ff6666'
GREEN_COLOR = '#99ff99'
ORANGE_COLOR = '#ffcc99'
YELLOW_COLOR = '#ffdb4d'
WHITE_FONT_COLOR = (255, 255, 255, 255)
RED_FONT_COLOR = (255, 102, 102, 255)
GREEN_FONT_COLOR = (153, 255, 153, 255)
ORANGE_FONT_COLOR = (255, 204, 153, 255)
YELLOW_FONT_COLOR = (255, 219, 77, 255)

TEMPERATURE_WARNING1 = 70
TEMPERATURE_WARNING2 = 80
TEMPERATURE_CAUTION = 90

PATH = os.path.dirname(os.path.realpath(__file__))
ICON_PATH = os.path.abspath(f"{PATH}/cpu.png")
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"

ICON_HEIGHT = 22
PADDING = 10

FONT_SIZE_FACTOR = 0.65
FONT_WIDTH_FACTOR = 9

image_to_show = None
old_image_to_show = None

def main():
    CPU_indicator = AppIndicator3.Indicator.new(APPINDICATOR_ID, ICON_PATH, AppIndicator3.IndicatorCategory.SYSTEM_SERVICES)
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
    global image_to_show
    global old_image_to_show

    # Generate disk info icon
    get_cpu_info()

    # Show pie chart
    icon_path = os.path.abspath(f"{PATH}/{image_to_show}")
    indicator.set_icon_full(icon_path, "disk usage")
    
    # Update old image path
    old_image_to_show = image_to_show

    return True

def get_cpu_info():
    global image_to_show
    global old_image_to_show

    # Get CPU temperatures
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

    # Load icon
    cpu_icon = Image.open(f'{PATH}/cpu.png')

    # Resize icon
    cpu_icon_relation = cpu_icon.width / cpu_icon.height
    cpu_icon_width = int(ICON_HEIGHT * cpu_icon_relation)
    scaled_cpu_icon = cpu_icon.resize((cpu_icon_width, ICON_HEIGHT), Image.LANCZOS)

    # New image with the combined icons
    if 'Tctl' in temperatures.keys():
        temp_to_str = temperatures['Tctl']
    elif 'Package id 0' in temperatures.keys():
        temp_to_str = temperatures['Package id 0']
    i_str = str(f" {temp_to_str}ºC")
    i_str_width = len(i_str) * FONT_WIDTH_FACTOR
    total_width = scaled_cpu_icon.width + i_str_width
    combined_image = Image.new('RGBA', (total_width, ICON_HEIGHT+PADDING), (0, 0, 0, 0))  # Transparent background

    # Combine icons
    cpu_icon_position = (0, int(PADDING/2))
    combined_image.paste(scaled_cpu_icon, cpu_icon_position, scaled_cpu_icon)

    # Create font object
    draw = ImageDraw.Draw(combined_image)
    font_size = int(ICON_HEIGHT * FONT_SIZE_FACTOR)
    font = ImageFont.truetype(FONT_PATH, font_size)

    # Set position of text
    text_position = (scaled_cpu_icon.width, int((ICON_HEIGHT + PADDING - font_size) / 2))

    # Draw text
    if temp_to_str < TEMPERATURE_WARNING1:
        used_color = WHITE_FONT_COLOR
    elif temp_to_str >= TEMPERATURE_WARNING1 and temp_to_str < TEMPERATURE_WARNING2:
        used_color = YELLOW_FONT_COLOR
    elif temp_to_str >= TEMPERATURE_WARNING2 and temp_to_str < TEMPERATURE_CAUTION:
        used_color = ORANGE_FONT_COLOR
    elif temp_to_str >= TEMPERATURE_CAUTION:
        used_color = RED_FONT_COLOR
    else:
        used_color = WHITE_FONT_COLOR
    draw.text(text_position, i_str, font=font, fill=used_color)

    # Save combined image
    timestamp = int(time.time())
    image_to_show = f'cpu_info_{timestamp}.png'
    combined_image.save(f'{PATH}/{image_to_show}')

    # Remove old image
    if os.path.exists(f'{PATH}/{old_image_to_show}'):
        os.remove(f'{PATH}/{old_image_to_show}')

    return temperatures

if __name__ == "__main__":
    # Remove all cpu_info_*.png files
    for file in os.listdir(PATH):
        if re.search(r'cpu_info_\d+.png', file):
            os.remove(f'{PATH}/{file}')

    signal.signal(signal.SIGINT, signal.SIG_DFL) # Allow the program to be terminated with Ctrl+C
    main()
