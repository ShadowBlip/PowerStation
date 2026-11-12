# Fan Control

PowerStation can control fans that have `pwmN` and `pwmN_enable` files in
hwmon. Fans start in automatic (firmware) mode and go back to it when
PowerStation stops. Userspace saves the user's profile and loads it again
after boot.

## Profiles

A profile sets a mode for each fan. Fans that are not listed use automatic.

```toml
name = "Quiet"

[[fans]]
fan = "ayaneo_ec/fan1"
mode = "software_curve"
sensors = ["k10temp/Tctl"]
curve = [[45, 0], [55, 15], [65, 25], [75, 35], [85, 55], [90, 75], [95, 100]]
```

A curve is a list of `[temperature in °C, fan percent]` points.

| Mode | Fields |
| --- | --- |
| `automatic` | None. The firmware controls the fan. |
| `manual` | `percent` (0-100) and `sensors` |
| `software_curve` | `curve` and `sensors`. Uses the hottest sensor. |
| `firmware_curve` | `curve` with one point per `pwmN_auto_pointM` in hwmon |

A fan runs at 100% when one of its sensors reaches 95°C.

Installed profiles are in `/usr/share/powerstation/platform/fan_curves/`.
A file with the same name in `/etc/powerstation/platform/fan_curves.d/`
replaces it. Installed profiles have a `[match]` section so they are only
listed on the right device:

```toml
[match]
dmi = { sys_vendor = "AYANEO", product_name = "AYANEO 3" }
device_tree = ["vendor,model"]
```

## DBus

`/org/shadowblip/Performance/Fan` (`org.shadowblip.Fan`)

| Name | Type | Description |
| --- | --- | --- |
| `LoadProfile(h)` | method | Load a TOML profile from an open file. |
| `LoadProfileJson(s)` | method | Load a profile from a JSON string. |
| `EnumerateFans()` | method | Returns a list of DBus paths to all fans. |
| `ProfileName` | `s` | Name of the loaded profile. |
| `ProfilePath` | `s` | Path of the loaded profile. Empty for JSON profiles. |
| `Profiles` | `as` | Paths of the installed profiles for this device. |
| `Sensors` | `as` | IDs of all temperature sensors. |

`/org/shadowblip/Performance/Fan/Fan0` (`org.shadowblip.Fan.Device`)

| Name | Type | Description |
| --- | --- | --- |
| `Id` | `s` | Fan ID used in profiles. |
| `AvailableModes` | `as` | Modes this fan supports. |
| `Mode` | `s` | Current mode of the fan. |
| `Percent` | `y` | Current fan speed from 0 to 100 percent. |

Example:

```bash
busctl call org.shadowblip.PowerStation /org/shadowblip/Performance/Fan \
  org.shadowblip.Fan LoadProfileJson s \
  '{"name":"Manual","fans":[{"fan":"ayaneo_ec/fan1","mode":"manual","percent":60,"sensors":["k10temp/Tctl"]}]}'
```
