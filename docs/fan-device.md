# Fan.Device DBus Interface API

## org.shadowblip.Fan.Device

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **AvailableModes** | *read* | *as* | Modes supported by the trusted platform configuration and discovered hardware. |
| **Fault** | *read* | *s* | Last controller failure, or empty after a successful profile load. |
| **Id** | *read* | *s* | Stable fan ID used in profiles. |
| **Mode** | *read* | *s* | Current mode: automatic, manual, software_curve, or firmware_curve. Suspend temporarily reports automatic while retaining the live profile in memory. |
| **Name** | *read* | *s* | Human-readable fan name. |
| **Percent** | *read* | *y* | Applied software duty from 0 to 100 percent. Read only when PercentValid is true; firmware duty is unavailable. |
| **PercentValid** | *read* | *b* | Whether Percent is available. False in Automatic, native firmware curves, suspend, and faults. |

### Methods

### Signals
