#!/usr/bin/env python3
"""Exercise the real daemon and systemd recovery using only fake fan files."""
import argparse
import json
import xml.etree.ElementTree as ET
import os
from pathlib import Path
import shlex
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import uuid

BUS = "org.shadowblip.PowerStation"
FAN = "/org/shadowblip/Performance/Fan/Fan0"
INTERFACE = "org.shadowblip.Fan.Device"
COLLECTION = "/org/shadowblip/Performance/Fan"
SENSOR = FAN + "/Sensor0"
SOURCE_ROOT = Path(__file__).resolve().parents[1]
LOGIN = "org.freedesktop.login1"
LOGIN_PATH = "/org/freedesktop/login1"
CONTROL = "org.shadowblip.Fan.TestLogind"


def run(*arguments, check=True, pass_fds=()):
    result = subprocess.run(arguments, text=True, capture_output=True, timeout=40, pass_fds=pass_fds)
    if check and result.returncode:
        raise RuntimeError(f"{shlex.join(arguments)} failed: {result.stderr.strip()}")
    return result


def wait(predicate, description, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if predicate():
                return
        except (RuntimeError, FileNotFoundError, ValueError):
            pass
        time.sleep(0.1)
    raise AssertionError(f"Timed out waiting for {description}")


class Harness:
    def __init__(self, root, binary, logind):
        self.root = root
        self.binary = binary
        self.logind = logind
        self.unit = f"powerstation-fan-test-{uuid.uuid4().hex}.service"
        self.processes = []
        self.logs = []
        self.assertions = 0
        self.started = False
        self.sysfs = root / "sys"
        self.platform = root / "platform"
        self.state = root / "state/fan-state.yaml"
        self.fan = self.sysfs / "class/hwmon/hwmon2"
        self.sensor = self.sysfs / "class/hwmon/hwmon9/temp1_input"

    def check(self, condition, message):
        if not condition:
            raise AssertionError(message)
        self.assertions += 1

    def write(self, relative, text):
        path = self.sysfs / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def bus(self, *arguments, check=True, pass_fds=()):
        return run("busctl", "--address", self.address, "--timeout=1", *arguments, check=check, pass_fds=pass_fds)

    def get(self, name, path=FAN, interface=INTERFACE):
        return shlex.split(self.bus("get-property", BUS, path, interface, name).stdout)[1]

    def current_profile(self):
        return self.get("CurrentProfileName", COLLECTION, "org.shadowblip.Fan")

    def load_fd(self, fd, check=True):
        return self.bus("call", BUS, COLLECTION, "org.shadowblip.Fan", "LoadProfile", "h", str(fd),
                        check=check, pass_fds=(fd,))

    def load_file(self, path=None, check=True):
        fd = os.open(path or self.profiles / "quiet.yaml", os.O_RDONLY | os.O_NONBLOCK)
        try:
            return self.load_fd(fd, check=check)
        finally:
            os.close(fd)

    def profile(self, mode, percent=None, check=True):
        fan = {"fan": "ayaneo-3/system", "mode": mode}
        if percent is not None:
            fan["percent"] = percent
        if mode == "software_curve":
            fan.update(sensors=["ayaneo-3/system/cpu"], curve=[[45, 0], [95, 100]])
        return self.call("LoadProfileJson", "s", json.dumps({"name": "Live", "fans": [fan]}), check=check)

    def call(self, name, signature=None, *arguments, check=True):
        args = ["call", BUS, COLLECTION, "org.shadowblip.Fan", name]
        if signature is not None:
            args += [signature, *arguments]
        return self.bus(*args, check=check)

    def systemctl(self, *arguments, check=True):
        return run("systemctl", "--user", *arguments, self.unit, check=check)

    def pid(self):
        return int(self.systemctl("show", "--property=MainPID", "--value").stdout.strip())

    def start_logind(self):
        log = (self.root / f"logind-{len(self.logs)}.log").open("w")
        self.logs.append(log)
        process = subprocess.Popen([str(self.logind), self.address, str(self.root)], stdout=log, stderr=log)
        self.processes.append(process)
        wait(lambda: self.bus("get-property", LOGIN, LOGIN_PATH, CONTROL, "ActiveInhibitors", check=False).returncode == 0,
             "private logind fixture")
        return process

    def setup(self):
        self.platform.mkdir()
        self.profiles = self.root / "profiles"
        self.overrides = self.root / "overrides"
        self.profiles.mkdir()
        self.overrides.mkdir()
        shutil.copyfile(SOURCE_ROOT / "rootfs/usr/powerstation/platform/fan_curves/quiet.yaml", self.profiles / "quiet.yaml")
        self.state.parent.mkdir()
        self.legacy = "schema_version: 1\nfans: {ayaneo-3/system: {mode: manual, manual_percent: 20}}\n"
        self.state.write_text(self.legacy)
        source = Path(__file__).resolve().parents[1] / "rootfs/usr/share/powerstation/platform/fan-ayaneo-3.yaml"
        self.configuration = self.platform / source.name
        shutil.copyfile(source, self.configuration)
        self.original_yaml = self.configuration.read_text()
        for path, value in {
            "class/dmi/id/sys_vendor": "AYANEO\n",
            "class/dmi/id/product_name": "AYANEO 3\n",
            "class/hwmon/hwmon2/name": "ayaneo_ec\n",
            "class/hwmon/hwmon2/device/uevent": "DRIVER=ayaneo-ec\nDEVPATH=/devices/platform/ayaneo-ec\n",
            "class/hwmon/hwmon2/pwm1": "128\n",
            "class/hwmon/hwmon2/pwm1_enable": "2\n",
            "class/hwmon/hwmon2/fan1_input": "1200\n",
            "class/hwmon/hwmon9/name": "k10temp\n",
            "class/hwmon/hwmon9/temp1_label": "Tctl\n",
            "class/hwmon/hwmon9/temp1_input": "50000\n",
        }.items():
            self.write(path, value)
        bus_log = (self.root / "dbus.log").open("w")
        self.logs.append(bus_log)
        daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"],
                                  stdout=subprocess.PIPE, stderr=bus_log, text=True)
        self.processes.append(daemon)
        self.address = daemon.stdout.readline().strip()
        if not self.address:
            raise RuntimeError("private D-Bus daemon did not start")
        self.login_process = self.start_logind()
        self.check(self.bus("call", LOGIN, LOGIN_PATH, "org.freedesktop.DBus.Properties",
                            "GetAll", "s", "org.freedesktop.login1.Manager", check=False).returncode != 0,
                   "fixture reproduces logind GetAll failure")
        self.check(self.bus("get-property", LOGIN, LOGIN_PATH, "org.freedesktop.login1.Manager",
                            "InhibitDelayMaxUSec", check=False).returncode == 0,
                   "inhibitor budget remains individually readable")

        unit_file = Path(__file__).resolve().parents[1] / "rootfs/usr/lib/systemd/system/powerstation.service"
        properties = []
        section = None
        for line in unit_file.read_text().splitlines():
            if line.startswith("["):
                section = line
            elif section == "[Service]" and "=" in line:
                key, value = line.split("=", 1)
                if key == "ExecStart":
                    continue
                if key == "ExecStopPost":
                    value = f"{self.binary} --restore-fans-auto"
                properties += [f"--property={key}={value}"]
        self.service_properties = properties
        self.start_service()
        self.ready()

    def start_service(self, check=True):
        # Inactive transient units are unloaded after a clean stop. Recreate
        # the same isolated service rather than starting an unloaded unit.
        self.systemctl("reset-failed", check=False)
        wait(lambda: self.systemctl("show", "--property=LoadState", "--value", check=False).stdout.strip() == "not-found",
             "previous transient unit to unload")
        self.started = True
        return run("systemd-run", "--user", "--quiet", f"--unit={self.unit}", *self.service_properties,
            f"--setenv=DBUS_SYSTEM_BUS_ADDRESS={self.address}",
            *([f"--setenv=XDG_DATA_DIRS={self.platform.parent.parent}"] if "nix/store" in str(self.platform)
              else [f"--setenv=POWERSTATION_FAN_PLATFORM_DIR={self.platform}"]),
            f"--setenv=POWERSTATION_FAN_SYSFS_ROOT={self.sysfs}",
            f"--setenv=POWERSTATION_FAN_STATE_PATH={self.state}",
            f"--setenv=POWERSTATION_FAN_DEVICE_TREE_PATH={self.root}/compatible", str(self.binary), check=check)

    def ready(self):
        wait(lambda: self.get("TelemetryValid", SENSOR, "org.shadowblip.Fan.Sensor") == "true", "validated fan telemetry")

    def mode(self, expected):
        wait(lambda: self.get("Mode") == expected, f"fan mode {expected}")

    def prepare(self, sleeping):
        outcome = self.root / "suspend-outcome"
        if sleeping:
            outcome.unlink(missing_ok=True)
        self.bus("call", LOGIN, LOGIN_PATH, CONTROL, "PrepareSleep", "b", str(sleeping).lower())
        if sleeping:
            wait(outcome.exists, "inhibitor release or expiry", timeout=5)
            return outcome.read_text()

    def check_bindings(self, write=False):
        def contract(raw, name):
            interface = ET.fromstring(raw).find(f"interface[@name='{name}']")
            assert interface is not None
            return {kind: sorted((node.attrib['name'], tuple(sorted(node.attrib.items())),
                                  tuple(tuple(sorted(arg.attrib.items())) for arg in node.findall('arg')))
                                 for node in interface.findall(kind))
                    for kind in ('method', 'property', 'signal')}

        for path, name, filename in [
            (COLLECTION, "org.shadowblip.Fan", "org-shadowblip-fan.xml"),
            (FAN, INTERFACE, "org-shadowblip-fan-device.xml"),
            (SENSOR, "org.shadowblip.Fan.Sensor", "org-shadowblip-fan-sensor.xml"),
        ]:
            live = self.bus("introspect", BUS, path, "--xml-interface").stdout
            self.check(all(interface in live for interface in ["org.freedesktop.DBus.Properties", "org.freedesktop.DBus.Introspectable", "org.freedesktop.DBus.Peer"]), f"standard D-Bus interfaces remain on {name}")
            binding = SOURCE_ROOT / "bindings/dbus-xml" / filename
            if write:
                # Keep zbus documentation comments, remove variable child paths,
                # and sort interfaces so regeneration has a stable ordering.
                parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
                node = ET.fromstring(live, parser=parser)
                for child in list(node):
                    if child.tag == "node":
                        node.remove(child)
                node[:] = sorted(node, key=lambda child: child.attrib.get("name", ""))
                ET.indent(node, space="  ")
                binding.write_text('<?xml version="1.0"?>\n' + ET.tostring(node, encoding="unicode") + '\n')
            self.check(contract(live, name) == contract(binding.read_text(), name), f"live {name} signatures match checked-in XML")

    def exercise_descriptors(self):
        arbitrary = self.root / "user-chosen.yaml"
        arbitrary.write_bytes((self.profiles / "quiet.yaml").read_bytes())
        link = self.root / "nix-store-profile.yaml"
        link.symlink_to(arbitrary)
        fd = os.open(link, os.O_RDONLY)
        try:
            os.lseek(fd, 0, os.SEEK_END)
            offset = os.lseek(fd, 0, os.SEEK_CUR)
            self.load_fd(fd)
            self.check(os.lseek(fd, 0, os.SEEK_CUR) == offset, "descriptor load preserves caller offset")
            self.check(self.get("CurrentProfilePath", COLLECTION, "org.shadowblip.Fan") == str(arbitrary), "descriptor path is informational")
            arbitrary.rename(self.root / "original.yaml")
            arbitrary.write_text("not a profile")
            self.load_fd(fd)
            self.check(self.current_profile() == "Quiet", "daemon reads open file instead of replaced caller path")
            (self.root / "original.yaml").write_text("invalid changed file")
            self.check(self.load_fd(fd, check=False).returncode != 0, "changed descriptor contents are validated")
        finally:
            os.close(fd)
        self.profile("automatic")
        for raw in ("not a profile", "x" * (65536 + 1)):
            arbitrary.write_text(raw)
            self.check(self.load_file(arbitrary, check=False).returncode != 0, "malformed or oversized descriptor is rejected")
            self.check(self.get("Mode") == "automatic", "invalid descriptor data causes no hardware writes")
        reader, writer = os.pipe()
        left, right = socket.socketpair()
        descriptors = [reader, writer, left.fileno(), os.open(self.root, os.O_RDONLY),
                       os.open("/dev/null", os.O_RDONLY), os.open(arbitrary, os.O_WRONLY), os.open(arbitrary, os.O_PATH)]
        try:
            for descriptor in descriptors:
                self.check(self.load_fd(descriptor, check=False).returncode != 0, "nonregular or unreadable descriptor rejected over D-Bus")
            self.check(self.call("LoadProfile", "h", "-1", check=False).returncode != 0, "invalid descriptor rejected by transport")
        finally:
            for descriptor in descriptors:
                if descriptor != left.fileno():
                    os.close(descriptor)
            left.close()
            right.close()
        print("PASS real descriptor passing, symlinks, replacement and invalid input", flush=True)

    def exercise(self):
        self.check(self.get("ApiVersion", COLLECTION, "org.shadowblip.Fan") == "2", "provisional revision marker")
        self.check(self.get("Mode") == "automatic", "fresh start ignores stale Manual")
        self.check(self.current_profile() == "", "startup has no selected profile")
        self.exercise_descriptors()
        self.load_file()
        self.check(self.get("Mode") == "software_curve", "Quiet selects software curve")
        self.check(int(self.get("Percent")) == 10, "Quiet uses running floor")
        self.profile("manual", 20)
        self.check((self.fan / "pwm1").read_text() == "51", "manual percentage converted internally")
        self.check(self.state.read_text() == self.legacy, "profile loads leave legacy state untouched")
        for raw in [
            {"name": "Bad", "fans": [{"fan": "ayaneo-3/system", "mode": "manual", "percent": 101}]},
            {"name": "Bad", "fans": [{"fan": "unknown", "mode": "automatic"}]},
            {"name": "Bad", "fans": [{"fan": "ayaneo-3/system", "mode": "software_curve", "sensors": ["unknown"], "curve": [[45, 0], [95, 100]]}]},
        ]:
            self.check(self.call("LoadProfileJson", "s", json.dumps(raw), check=False).returncode != 0, "invalid profile rejected")
        self.check(self.get("Mode") == "manual" and (self.fan / "pwm1").read_text() == "51", "invalid requests preserve active profile and hardware")
        print("PASS profiles, percentages and invalid requests", flush=True)

        self.check(self.prepare(True) == "released", "successful handoff releases inhibitor")
        self.check((self.fan / "pwm1_enable").read_text() == "2", "suspend hands control to firmware")
        self.check(self.get("Mode") == "automatic", "current hardware mode is automatic during sleep")
        self.check(self.current_profile() == "Live", "live profile retained during sleep")
        self.check(self.get("TelemetryValid", SENSOR, "org.shadowblip.Fan.Sensor") == "false", "suspended sensor reading invalid")
        self.check(self.get("PercentValid") == "false", "suspended duty unavailable")
        self.check(self.profile("manual", 50, check=False).returncode != 0, "suspended Manual rejected")
        self.check(self.profile("software_curve", check=False).returncode != 0, "suspended curve rejected")
        self.check(self.load_file(check=False).returncode != 0, "suspended file load rejected")
        new_fan = self.fan.with_name("hwmon7")
        self.fan.rename(new_fan)
        self.fan = new_fan
        new_sensor = self.sensor.parent.with_name("hwmon11")
        self.sensor.parent.rename(new_sensor)
        self.sensor = new_sensor / "temp1_input"
        self.prepare(False)
        self.ready()
        self.mode("manual")
        self.check((self.fan / "pwm1_enable").read_text() == "1", "resume restores live selection at rediscovered paths")
        self.check(self.current_profile() == "Live", "resume preserves profile identity")
        print("PASS suspend guards, live profile and fresh resume discovery", flush=True)

        self.sensor.unlink()
        wait(lambda: self.get("TelemetryValid", SENSOR, "org.shadowblip.Fan.Sensor") == "false", "sensor loss invalidation")
        self.mode("automatic")
        self.check(self.get("Fault") != "", "sensor loss reports fault")
        self.check(self.get("PercentValid") == "false", "fault duty unavailable")
        self.check(self.current_profile() == "", "fault clears current profile identity")
        self.profile("automatic")
        self.check((self.fan / "pwm1_enable").read_text() == "2", "Automatic works without telemetry")
        self.sensor.write_text("50000\n")
        self.ready()
        print("PASS sensor loss, Automatic and telemetry recovery", flush=True)

        self.load_file()
        self.prepare(True)
        self.profile("automatic")
        self.check(self.profile("manual", 50, check=False).returncode != 0, "custom remains gated after suspended Automatic")
        self.prepare(False)
        self.ready()
        self.mode("automatic")
        self.check((self.fan / "pwm1_enable").read_text() == "2", "Automatic during suspend cancels live custom restoration")

        for legacy in [self.legacy, "not: [valid\n"]:
            self.profile("manual", 20)
            self.systemctl("stop")
            self.check((self.fan / "pwm1_enable").read_text() == "2", "clean stop restores firmware")
            self.state.write_text(legacy)
            self.start_service()
            self.ready()
            self.mode("automatic")
            self.check((self.fan / "pwm1_enable").read_text() == "2", "clean restart ignores valid or malformed legacy state")
            self.check(self.state.read_text() == legacy, "legacy file unchanged on clean restart")
        self.legacy = self.state.read_text()
        self.state.parent.chmod(0o500)
        try:
            self.profile("manual", 50)
            self.check(self.get("Mode") == "manual", "unwritable legacy state cannot fail profile load")
        finally:
            self.state.parent.chmod(0o700)
        print("PASS clean restarts and ignored legacy persistence", flush=True)

        for name, signum in [("SIGKILL", signal.SIGKILL), ("SIGSTOP/watchdog", signal.SIGSTOP)]:
            self.profile("manual", 20)
            old_pid = self.pid()
            os.kill(old_pid, signum)
            wait(lambda: self.pid() not in (0, old_pid), f"restart after {name}")
            self.ready()
            self.mode("automatic")
            self.check((self.fan / "pwm1_enable").read_text() == "2", f"{name} returns to firmware")
            self.check(self.state.read_text() == self.legacy, f"{name} leaves malformed legacy state untouched")
            self.check(self.current_profile() == "", f"{name} does not replay profile")
            print(f"PASS {name} recovery", flush=True)

        self.profile("manual", 20)
        self.login_process.terminate()
        self.login_process.wait(timeout=5)
        wait(lambda: (self.fan / "pwm1_enable").read_text() == "2", "fallback after logind owner loss")
        self.check(self.state.read_text() == self.legacy, "owner-loss recovery leaves legacy state untouched")
        self.systemctl("stop", check=False)
        self.login_process = self.start_logind()
        self.start_service()
        self.ready()
        print("PASS logind owner loss", flush=True)

        self.profile("manual", 20)
        enable = self.fan / "pwm1_enable"
        enable.unlink()
        enable.mkdir()
        self.check(self.prepare(True) in ("released", "expired"), "failed handoff cannot indefinitely delay sleep")
        self.check(self.profile("manual", 50, check=False).returncode != 0, "failed handoff rejects custom control")
        enable.rmdir()
        enable.write_text("2\n")
        self.systemctl("stop", check=False)
        self.prepare(False)
        self.start_service()
        self.ready()
        print("PASS bounded failed handoff", flush=True)

        self.systemctl("stop")
        self.configuration.write_text("not: [valid\n")
        self.start_service(check=False)
        wait(lambda: self.systemctl("show", "--property=ExecMainStatus", "--value").stdout.strip() == "1", "malformed hardware YAML startup failure")
        self.check(self.profile("manual", 50, check=False).returncode != 0, "malformed hardware configuration cannot expose healthy service")
        self.systemctl("stop", check=False)
        self.configuration.write_text(self.original_yaml)
        print("PASS malformed hardware configuration rejection", flush=True)
        # XDG discovery supports system profiles outside /usr, including Nix.
        xdg = self.root / "nix/store/test-powerstation/share"
        platform = xdg / "powerstation/platform"
        platform.parent.mkdir(parents=True)
        platform.symlink_to(self.platform)
        self.platform = platform
        self.systemctl("stop", check=False)
        for field in ("sys_vendor", "product_name"):
            (self.sysfs / "class/dmi/id" / field).unlink()
        self.configuration.write_text(self.original_yaml.replace("    dmi:", "    device_tree: ['vendor,test-handheld']\n    dmi:"))
        (self.root / "compatible").write_bytes(b"vendor,test-handheld\0vendor,test-soc\0")
        self.start_service()
        self.ready()
        self.mode("automatic")
        self.profile("manual", 20)
        self.check(self.get("Mode") == "manual", "Device Tree only discovery supports ordinary profile requests")
        self.systemctl("stop")
        self.check((self.fan / "pwm1_enable").read_text() == "2", "Device Tree recovery helper restores Automatic")
        print("PASS Device Tree only startup and recovery through relocated platform data", flush=True)
        print(f"PASS lifecycle harness: {self.assertions} assertions", flush=True)

    def close(self):
        if self.started:
            self.systemctl("stop", check=False)
            self.systemctl("reset-failed", check=False)
        for process in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        for log in self.logs:
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=lambda value: Path(value).resolve())
    parser.add_argument("logind", type=lambda value: Path(value).resolve())
    parser.add_argument("--write-bindings", action="store_true", help="regenerate fan XML from the isolated running service")
    arguments = parser.parse_args()
    for command in ("systemd-run", "systemctl", "busctl", "dbus-daemon"):
        if shutil.which(command) is None:
            parser.error(f"missing command: {command}")
    for binary in (arguments.binary, arguments.logind):
        if not binary.is_file():
            parser.error(f"missing built binary: {binary}")
    run("systemctl", "--user", "list-units", "--no-pager", "--no-legend")
    with tempfile.TemporaryDirectory(prefix="powerstation-fan-lifecycle-") as directory:
        harness = Harness(Path(directory), arguments.binary, arguments.logind)
        try:
            harness.setup()
            harness.check_bindings(write=arguments.write_bindings)
            harness.exercise()
        except Exception:
            if harness.started:
                result = run("journalctl", "--user", "--unit", harness.unit, "--no-pager", "-n", "30", check=False)
                print(result.stdout, flush=True)
            raise
        finally:
            harness.close()


if __name__ == "__main__":
    main()
