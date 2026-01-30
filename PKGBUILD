# Maintainer: Ekrem Karaca <ekrem.karaca@yandex.com>

pkgname=nwg-panel-rs-bin
pkgver=0.0.3
pkgrel=1
pkgdesc="Rewrite of nwg-panel in Rust with GTK4 (prebuilt binary)"
arch=('x86_64')
url="https://github.com/ekremkaraca/nwg-panel-rs"
license=('MIT')

depends=(
  'gtk4'
  'gtk4-layer-shell'
  'dbus'
  'hyprland'
)

optdepends=(
  'brightnessctl: brightness slider backend (recommended)'
  'light: brightness slider backend (alternative)'
  'pamixer: volume slider backend (recommended)'
  'pulseaudio-utils: provides pactl volume fallback'
  'upower: battery info backend'
)

provides=('nwg-panel-rs')
conflicts=('nwg-panel-rs')
install="nwg-panel-rs-bin.install"

_target="x86_64-unknown-linux-gnu"
source=("$pkgname-$pkgver.tar.gz::$url/releases/download/v$pkgver/nwg-panel-rs-$pkgver-$_target.tar.gz")

sha256sums=('SKIP')

package() {
  install -Dm755 "nwg-panel-rs" "$pkgdir/usr/bin/nwg-panel-rs"
}