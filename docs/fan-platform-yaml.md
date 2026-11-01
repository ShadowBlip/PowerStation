# Fan profiles

The fan interface is experimental. A profile selects a mode for every enumerated fan, using stable fan and sensor IDs.

```yaml
name: Quiet
fans:
  - fan: ayaneo-3/system
    mode: software_curve
    sensors: [ayaneo-3/system/cpu]
    curve: [[45, 0], [55, 15], [65, 25], [75, 35], [85, 55], [90, 75], [95, 100]]
```

Clients open YAML files for [`LoadProfile`](fan.md#loadprofile), or send JSON through [`LoadProfileJson`](fan.md#loadprofilejson):

```json
{"name":"Manual","fans":[{"fan":"ayaneo-3/system","mode":"manual","percent":60}]}
```

| Mode | Required fields and behavior |
| --- | --- |
| `automatic` | No additional fields; firmware controls cooling. |
| `manual` | `percent`: fixed duty from 0–100%, subject to the platform's running floor. |
| `software_curve` | `sensors` and `curve`: interpolate using the hottest selected sensor. |
| `firmware_curve` | `sensors` and `curve`: program the driver's native curve. |

Use each fan's `AvailableModes`; check `PercentValid` and sensor `TelemetryValid` before displaying readings. Percentages are control duty, not RPM.

## Validation

- Specify every fan exactly once. Reject unknown IDs, duplicate sensors, unsupported modes and fields inappropriate for the mode.
- Curves need at least two points, finite and strictly increasing temperatures within platform limits and at most 95°C, and nondecreasing integer percentages from 0–100, ending at 100%.
- Device percentage limits apply. Software control enforces the running floor and stop/restart hysteresis. Native curves must stay at or above that floor and use exactly the driver's point count, representable temperatures/PWM values and designated sensor.
- Selected sensors must belong to the fan and be available. Required safety sensors remain service-controlled.

Requests run sequentially. The whole profile is validated before writes; an application failure returns every affected fan to Automatic and reports failure.

## Files and recovery

For packaged profiles, clients check `/etc/powerstation/platform/fan_curves.d/` before `/usr/powerstation/platform/fan_curves/`. Same-name replacements override whole files. Explicit user paths select that exact file. Nonstandard installations must configure the client's profile directory.

Every service process starts Automatic. Applications save and reload preferences through ordinary requests. Successful suspend/resume retains the live profile; faults return control to firmware. Automatic remains available without telemetry. The provisional 95°C full-speed / 98°C firmware-handoff policy requires physical acceptance; firmware handoff does not establish stronger cooling.

## Platform configuration

Trusted `fan-*.yaml` files use the first existing `powerstation/platform` directory in the service's XDG data directories, falling back to `/usr/share/powerstation/platform`; `POWERSTATION_FAN_PLATFORM_DIR` overrides it. The administrator controls these files and the service environment. Profiles cannot change hardware paths or safety policy.

See the [packaged hardware definition](../rootfs/usr/share/powerstation/platform/fan-ayaneo-3.yaml). DMI alternatives match their fields together. `device_tree: ["vendor,model"]` matches exact NUL-separated `/proc/device-tree/compatible` entries. Either identity source may match; multiple matches are rejected. Adding a platform requires driver/device validation.
