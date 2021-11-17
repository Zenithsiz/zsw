set -e

#cargo run --release -- \
#	~/.wallpaper/test \
#	--image-duration 5.0 \
#	--window-geometry "3280x1080+0+0" \
#	--image-geometry "1360x768+0+312" \
#	--image-geometry "1920x1080+1360+0" \
#	--fade-point 0.8 \
#	--image-backlog 0 \

cargo run -- \
	~/.wallpaper/active \
	--image-duration 5.0 \
	--window-geometry "3280x1080+0+0" \
	--image-geometry "1920x1080+1360+0" \
	--fade-point 0.8 \
	--image-backlog 0 \
	--loader-threads 4 \

#cargo run -- \
#	~/.wallpaper/active \
#	--image-duration 5.0 \
#	--window-geometry "3280x1080+0+0" \
#	--image-geometry "960x540+1360+0" \
#	--image-geometry "960x540+1360+540" \
#	--image-geometry "960x540+2320+0" \
#	--image-geometry "960x540+2320+540" \
#	--fade-point 0.8 \
#	--image-backlog 0 \

#cargo run --release -- \
#	~/.wallpaper/active \
#	--image-duration 5.0 \
#	--window-geometry "3280x1080+0+0" \
#	--grid "2x3@1920x1080+1360+0" \
#	--fade-point 0.8 \
#	--image-backlog 0 \