#!/bin/bash



launchctl unload com.joonho.rss.plist && launchctl stop com.joonho.rss.plist
launchctl load com.joonho.rss.plist
launchctl start com.joonho.rss.plist
launchctl print gui/$(id -u)/com.joonho.rss | grep state
