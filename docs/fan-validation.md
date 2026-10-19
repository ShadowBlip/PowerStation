# Fan validation

Use Rust 1.88 and the lockfile. The build image supplies native libraries and both targets.

```sh
cargo fmt --all -- --check
cargo test --locked --all
cargo clippy --locked --all-targets -- -D warnings -A clippy::uninlined_format_args -A clippy::nonminimal_bool
cargo build --locked --release --target x86_64-unknown-linux-gnu
cargo build --locked --release --target aarch64-unknown-linux-gnu
make in-docker TARGET='test test-fan-lifecycle-build'
make test-fan-lifecycle
```

The lifecycle harness needs user systemd, `dbus-daemon`, `busctl` and `libpci`. It runs the daemon against fake sysfs and private D-Bus/logind, covering descriptors, recovery and lifecycle failures without controlling host fans or suspending the host. It also compares live custom D-Bus signatures with checked-in XML.

To regenerate bindings and documentation:

```sh
python3 tests/fan_lifecycle.py target/x86_64-unknown-linux-gnu/debug/powerstation target/x86_64-unknown-linux-gnu/debug/examples/fan_test_logind --write-bindings
make docs
```

Inspect both architectures' archives/RPMs for paths, permissions, binary bytes and runtime dependencies. Test relocation with `make install PREFIX=/chosen/prefix DESTDIR=/staging NO_RELOAD=true`. Configure service XDG directories accordingly; distribution units must retain notify/watchdog settings and the `--restore-fans-auto` stop hook.

Simulation does not qualify physical cooling, 95°C/98°C policy, native curves, HX 370, revised-device suspend, NixOS integration or ARM RPM runtime dependencies. Record results against the tested revision; historical device results do not validate revised code.
