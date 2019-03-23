# wlines_run

A simple Windows application-launcher for use with [wlines](https://github.com/JerwuQu/wlines).

### Usage

1. Make sure `wlines.exe` is in your PATH

2. Run `wlines_run.exe index` to create an index of your start-menu folder and PATH

3. Run `wlines_run.exe run` - any additional arguments are passed to `wlines`

4. **Optional:** Rebind your Win-key to run `wlines_run.exe run` instead of the default start-menu

### Build steps

1. [Install cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) if you haven't yet

2. `cargo build --release`

### License

This project is licensed under the GNU General Public License v3.0. See LICENSE for more details.

