# Fan.Sensor DBus Interface API

## org.shadowblip.Fan.Sensor

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **Id** | *read* | *s* | Stable sensor ID used in profile sensor lists. |
| **Name** | *read* | *s* | Human-readable sensor name. |
| **TelemetryValid** | *read* | *b* | Whether a valid reading is available. False during suspend and after telemetry loss. |
| **TemperatureC** | *read* | *d* | Last validated temperature in degrees Celsius. Read only when TelemetryValid is true. |

### Methods

### Signals
