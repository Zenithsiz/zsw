#!/bin/env bash

set -e

#cargo run -- \
#	~/.wallpaper/active \
#	--image-duration 15.0 \
#	--window-geometry "3280x1080+0+0" \
#	--panel-geometry "1920x1080+1360+0" \
#	--fade-point 0.8 \
#	--image-backlog 4 \
#	--image-loader-args "image-loader-args.json"

# "1360x768+0+312"

#cargo run -- \
#	~/.wallpaper/active \
#	--image-duration 5.0 \
#	--window-geometry "3280x1080+0+0" \
#	--panel-geometry "960x540+1360+0" \
#	--panel-geometry "960x540+1360+540" \
#	--panel-geometry "960x540+2320+0" \
#	--panel-geometry "960x540+2320+540" \
#	--fade-point 0.8 \
#	--image-backlog 0 \

cargo run -- \
	~/.wallpaper/active \
	--image-duration 5.0 \
	--window-geometry "1920x1080+1360+0" \
	--fade-point 0.85 \
	--image-backlog 4 \
	--image-loader-args "image-loader-args.json"
