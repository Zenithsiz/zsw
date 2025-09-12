# Maintainer: Filipe Rodrigues <filipejacintorodrigues1@gmail.com>
pkgname=zsw
pkgver=0.1.0
pkgrel=1
pkgdesc="Zenithsiz's scrolling wallpaper"
arch=('x86_64')
url="https://github.com/zenithsiz/zsw"
depends=('gcc-libs')
makedepends=('cargo-nightly')
source=("$pkgname-$pkgver.tar.gz::https://github.com/zenithsiz/$pkgname/archive/$pkgver.tar.gz")
sha512sums=('e0a89d3c65572cc50314c641764ab5dbafaa5610973f7bf9e393b99434250229d2e45b8de3598050b69a09880263691c89cdc99a0de4b836a41b0a39d6f45aa3')

prepare() {
	cd "$pkgname-$pkgver"

	export RUSTUP_TOOLCHAIN=nightly
	cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
	cd "$pkgname-$pkgver"

	# TODO: Remove adding `tokio_unstable` to `RUSTFLAGS` once
	#       we can get `cargo` to read `.cargo/config.toml` here
	export RUSTUP_TOOLCHAIN=nightly
	export RUSTFLAGS+=" --cfg tokio_unstable"
	cargo build --frozen --release
}

check() {
	cd "$pkgname-$pkgver"

	# TODO: Remove adding `tokio_unstable` to `RUSTFLAGS` once
	#       we can get `cargo` to read `.cargo/config.toml` here
	export RUSTUP_TOOLCHAIN=nightly
	export RUSTFLAGS+=" --cfg tokio_unstable"
	cargo test --frozen
}

package() {
	cd "$pkgname-$pkgver"

	install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/$pkgname"
	install -Dm0644 -t "$pkgdir/usr/share/applications/" "install/zsw.desktop"
}
