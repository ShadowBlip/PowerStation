# Fan profiles and platform configuration

The fan interface is experimental and has not been merged. `ApiVersion = 2` is a provisional revision marker, not a stable compatibility promise. See [the methods](fan.md), [fan properties](fan-device.md) and [sensor properties](fan-sensor.md).

## Profiles

A profile names a selection for each controlled fan. Use `EnumerateFans` and `EnumerateSensors` to find stable IDs and available modes.

```yaml
name: Manual
fans:
  - fan: ayaneo-3/system
    mode: manual
    percent: 60
```

```yaml
name: Quiet
fans:
  - fan: ayaneo-3/system
    mode: software_curve
    sensors: [ayaneo-3/system/cpu]
    curve: [[45, 0], [55, 15], [65, 25], [75, 35], [85, 55], [90, 75], [95, 100]]
```

| Mode | Behavior | Profile fields |
| --- | --- | --- |
| `automatic` | Let the firmware control cooling. | No percentage, sensors or curve. |
| `manual` | Request a fixed control percentage. | `percent` from 0 to 100, subject to the platform's minimum running percentage. |
| `software_curve` | The service interpolates the curve using the hottest selected sensor. | `sensors` and `curve`. |
| `firmware_curve` | Program the curve supported by the driver. | `sensors` and `curve` matching the native hardware constraints. |

Only modes listed in each fan's `AvailableModes` are supported. Percentages describe control duty; they are not measured RPM. Check `PercentValid` and sensor `TelemetryValid` before displaying values.

Curves require at least two points. Temperatures must be finite, strictly increasing, within the platform's valid range and no higher than 95°C. Percentages must be integers from 0 to 100, nondecreasing, and meet the platform's running floor. The final point must be 100%. Zero is accepted only when the platform permits stopping and the point is within its stop-temperature limit. Sensor IDs must be known, unique, associated with that fan and available. A native curve must use the platform's designated hardware sensor, contain exactly the driver's number of points, and satisfy its integer temperature/PWM representation and bounds. Hardware validation also checks required safety sensors independently of the selected curve sensors.

`LoadProfile(h)` reads a YAML profile from a file descriptor opened by the caller. It accepts readable regular files, including files opened through symlinks or outside packaged directories. It rejects pipes, sockets, devices, directories and unreadable descriptors. Input is limited to 64 KiB and copied before validation and hardware writes. The service never reopens the caller's path. `CurrentProfilePath` is informational descriptor metadata and is empty when unavailable.

`LoadProfileJson(s)` accepts the equivalent JSON profile, with the same size and validation limits:

```json
{"name":"Automatic","fans":[{"fan":"ayaneo-3/system","mode":"automatic"}]}
```

The complete profile is validated before hardware writes. Unknown or duplicate IDs, unsupported modes and invalid fields are rejected. If application fails after writes begin, every affected fan is returned to Automatic and the call reports failure. Requests are serialized through the profile transaction lock; clients do not acquire permanent ownership.

The packaged Quiet profile is `/usr/powerstation/platform/fan_curves/quiet.yaml`. Clients choosing a packaged name first check `/etc/powerstation/platform/fan_curves.d/` for a same-name replacement. A replacement overrides the whole profile; fields are never merged. An explicit path selects that exact file. Clients open the selected file and pass its descriptor. Installations using another prefix must expose or resolve their corresponding profile directory in the client.

## Startup and recovery

Every service process starts in firmware Automatic. Applications own saving preferences and may restore a profile through an ordinary request at application startup. The service does not prevent a client from restoring after a service restart. There are no startup records, boot-attempt fences or guarded restoration methods.

Successful suspend/resume retains the live profile in memory, hands control to firmware before sleep, rediscovers hardware on resume and restores the profile after protection and telemetry recover. Custom requests are rejected during lifecycle restrictions. Automatic remains callable without valid telemetry. Sensor loss, stalls, unexpected driver mode changes, logind loss and watchdog recovery return control to firmware; faults clear the active profile identity. A driver-mode check can detect some external changes but does not provide exclusive sysfs access.

The service owns the provisional 95°C full-speed and 98°C firmware-handoff policy. Firmware handoff transfers cooling responsibility; it does not prove stronger cooling. Physical thermal acceptance remains required.

## Trusted platform configuration

Hardware configuration is administrator-controlled. Profiles cannot choose sysfs paths, driver mode numbers, required safety sensors or weaker critical thresholds.

Fan platform files use `fan-*.yaml` in the first existing `powerstation/platform` directory in the service's XDG data directories, with `/usr/share/powerstation/platform` as fallback. `POWERSTATION_FAN_PLATFORM_DIR` explicitly overrides that directory. The service environment and these files must be controlled by the administrator.

A device can match DMI alternatives, Device Tree compatible strings, or both:

```yaml
schema_version: 1
devices:
  - id: example
    name: Example handheld
    dmi:
      - sys_vendor: Example
        product_name: Handheld
    device_tree: ["example,handheld"]
    fans: [] # Replace with the platform's validated fan definitions.
```

DMI fields within an alternative are ANDed; alternatives are ORed. `device_tree` is a list of exact strings from the NUL-separated `/proc/device-tree/compatible` property. Either identity source can match a device. Multiple matching devices are rejected, including cross-DMI/Device-Tree ambiguity. Missing identity matches nothing.

Each fan definition specifies its hwmon name, parent driver, channel, driver mode values, PWM range/write order, temperature sources and operating limits. Native curve support additionally requires a complete set of driver points and a designated hardware sensor. See [the packaged AYANEO configuration](../rootfs/usr/share/powerstation/platform/fan-ayaneo-3.yaml) for the schema in use. Adding a platform requires driver and device validation; a matching identifier alone does not establish safe control.
