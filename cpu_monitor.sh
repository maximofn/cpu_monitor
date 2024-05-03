#!/usr/bin/bash
# Get script path
SCRIPT_PATH=$(dirname $0)
/usr/bin/python3 $SCRIPT_PATH/cpu_monitor.py >cpu_monitor.log 2>cpu_monitor_error.log