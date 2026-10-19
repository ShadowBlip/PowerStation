# Fan validation

Use Rust 1.88 and the checked-in lockfile. The build image supplies the native libraries and both cross-compilation targets.

```sh
cargo fmt --all -- --check
cargo test --locked --all
cargo clippy --locked --all-targets -- -D warnings -A clippy::uninlined_format_args -A clippy::nonminimal_bool
cargo build --locked --release --target x86_64-unknown-linux-gnu
cargo build --locked --release --target aarch64-unknown-linux-gnu
make in-docker TARGET='test test-fan-lifecycle-build'
make test-fan-lifecycle
```

The lifecycle harness requires a working user systemd manager, `dbus-daemon`, `busctl` and `libpci`. It starts the real daemon with fake sysfs, a private bus and a private logind fixture. It never suspends the host or controls host fans. It checks descriptor passing and invalid inputs, fresh/clean/crash/watchdog startup in Automatic, ignored legacy state, sensor loss, suspend/resume, bounded handoff, logind owner loss, individual-property reads when logind `GetAll` fails, and relocated platform discovery. Unit tests cover complete-profile validation, multi-fan rollback and native curve constraints.

The harness compares live custom D-Bus signatures with the checked-in XML. Regenerate XML after changing the implementation:

```sh
python3 tests/fan_lifecycle.py target/x86_64-unknown-linux-gnu/debug/powerstation target/x86_64-unknown-linux-gnu/debug/examples/fan_test_logind --write-bindings
make docs
```

Build archives and RPMs in an isolated output directory. Inspect their architecture, paths, ownership, modes, binary bytes and runtime dependencies before distribution. `make install PREFIX=/chosen/prefix DESTDIR=/staging NO_RELOAD=true` separates runtime paths from staging paths. Configure the service's XDG data directories when installing outside the default prefix. If a distribution supplies its own unit (as NixOS does), it must also carry the notify/watchdog settings and the `--restore-fans-auto` stop hook from the packaged unit; starting the binary alone does not supply out-of-process recovery.

Simulation does not qualify physical cooling, native hardware curves or suspend behavior on a device. The 95°C/98°C policy needs physical acceptance. NixOS service/module behavior, revised-device tests, official image integration and ARM RPM dependency/runtime qualification require their own environments. Record results against the tested source revision; historical device results do not validate a revised implementation.
