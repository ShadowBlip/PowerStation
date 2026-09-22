# Fan.Device DBus Interface API

## org.freedesktop.DBus.Properties

### Methods

#### Get

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* | |
| **property_name** | *in* | *s* | |
| \*\*\*\* | *out* | *v* | |

#### Set

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* | |
| **property_name** | *in* | *s* | |
| **value** | *in* | *v* | |

#### GetAll

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* | |
| \*\*\*\* | *out* | *a{sv}* | |

### Signals

#### PropertiesChanged

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | \*\* | *s* | |
| **changed_properties** | \*\* | *a{sv}* | |
| **invalidated_properties** | \*\* | *as* | |

## org.freedesktop.DBus.Introspectable

### Methods

#### Introspect

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| \*\*\*\* | *out* | *s* | |

### Signals

## org.freedesktop.DBus.Peer

### Methods

#### Ping

#### GetMachineId

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| \*\*\*\* | *out* | *s* | |

### Signals

## org.shadowblip.Fan.Device

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **ActivePreset** | *read* | *s* | |
| **ApiVersion** | *read* | *u* | |
| **AutomaticVerified** | *read* | *b* | |
| **AvailableModes** | *read* | *as* | |
| **Backend** | *read* | *s* | |
| **Controlling** | *read* | *b* | |
| **Curve** | *read* | *a(dy)* | |
| **CurvePointCount** | *read* | *q* | |
| **EffectivePercent** | *read* | *n* | |
| **EmergencyHandoffTemperatureC** | *read* | *d* | |
| **Fault** | *read* | *s* | |
| **FullSpeedTemperatureC** | *read* | *d* | |
| **Id** | *read* | *s* | |
| **ManualPercent** | *read* | *y* | |
| **MaximumValidRpm** | *read* | *u* | |
| **MaximumValidTemperatureC** | *read* | *d* | |
| **MinimumPercent** | *read* | *y* | |
| **MinimumRunningPercent** | *read* | *y* | |
| **MinimumValidTemperatureC** | *read* | *d* | |
| **Mode** | *read* | *s* | |
| **Name** | *read* | *s* | |
| **Presets** | *read* | *as* | |
| **PwmMax** | *read* | *u* | |
| **PwmMin** | *read* | *u* | |
| **PwmReadbackTolerance** | *read* | *u* | |
| **RestartTemperatureC** | *read* | *d* | |
| **Rpm** | *read* | *u* | |
| **StallGraceSeconds** | *read* | *u* | |
| **StallRpm** | *read* | *u* | |
| **StopTemperatureC** | *read* | *d* | |
| **Suspended** | *read* | *b* | |
| **TelemetryValid** | *read* | *b* | |
| **TemperatureC** | *read* | *d* | |

### Methods

#### SetAutomatic

#### SetManual

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **percent** | *in* | *y* | |

#### SetCurve

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **points** | *in* | *a(dy)* | |

#### ApplyPreset

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **name** | *in* | *s* | |

### Signals
