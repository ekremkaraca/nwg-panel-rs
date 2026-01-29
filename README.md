# nwg-panel-rs

Rewrite of nwg-panel in Rust with GTK4. This project is still in development and not ready for production use. Your feedback and contributions are welcome!

## Install

### Prebuilt (recommended)

Arch Linux users can install the prebuilt binary from the AUR as `nwg-panel-rs-bin`.

Example:

```bash
paru -S nwg-panel-rs-bin
```

Suggested Arch package dependencies for the AUR `PKGBUILD`:

- **depends**: `gtk4`, `gtk4-layer-shell`, `dbus`, `hyprland`
- **makedepends**: `tar` (if repackaging release tarballs)

### From source

```bash
cargo install --path .
```

## Releases

- Tag versions as `vX.Y.Z` (example: `v0.0.1`).
- GitHub Actions will build and upload a prebuilt tarball per supported target.
- The workflow is at `.github/workflows/release.yml`.

Currently supported targets:

- `x86_64-unknown-linux-gnu`

## Build

```bash
cargo build
```
See [Developer Notes](docs/DEV_NOTES.md) for more details.


## Thanks

- [nwg-panel](https://github.com/nwg-piotr/nwg-panel)
- [gtk4-rs](https://gtk-rs.org/)
- [windsurf ai](https://windsurf.com)

Crates:

- [anyhow](https://crates.io/crates/anyhow)
- [chrono](https://crates.io/crates/chrono)
- [clap](https://crates.io/crates/clap)
- [crossbeam-channel](https://crates.io/crates/crossbeam-channel)
- [dirs](https://crates.io/crates/dirs)
- [gdk-pixbuf](https://crates.io/crates/gdk-pixbuf)
- [gdk4](https://crates.io/crates/gdk4)
- [gio](https://crates.io/crates/gio)
- [glib](https://crates.io/crates/glib)
- [gtk4-layer-shell](https://crates.io/crates/gtk4-layer-shell)
- [hyprlang](https://crates.io/crates/hyprlang)
- [hyprland](https://crates.io/crates/hyprland)
- [serde](https://crates.io/crates/serde)
- [serde_json](https://crates.io/crates/serde_json)
- [zbus](https://crates.io/crates/zbus)

## License

MIT License

Copyright (c) 2021 - Piotr Miller & Contributors

Copyright (c) 2026 - nwg-panel-rs & Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.