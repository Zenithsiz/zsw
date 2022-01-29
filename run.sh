#!/bin/env bash

set -e

cargo run -- \
	~/.wallpaper/active \
	--image-duration 30.0 \
	--window-geometry "3280x1080+0+0" \
	--panel-geometry "1920x1080+1360+0" \
	--panel-geometry "1360x768+0+312" \
	--fade-point 0.8

#cargo run --release -- \
#	~/.wallpaper/active \
#	--image-duration 5.0 \
#	--window-geometry "1920x1080+1360+0" \
#	--grid "3x3@1920x1080+0+0" \
#	--fade-point 0.8

#cargo run -- \
#	~/.wallpaper/test \
#	--image-duration 1.0 \
#	--window-geometry "1920x1080+1360+0" \
#	--fade-point 0.85
