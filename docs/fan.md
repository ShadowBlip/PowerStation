# Fan DBus Interface API

## org.shadowblip.Fan

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **ApiVersion** | *read* | *u* | Provisional interface revision; this fan interface is not yet stable. |
| **CurrentProfileName** | *read* | *s* | Active profile name, including during successful suspend. Empty after startup or a controller fault. |
| **CurrentProfilePath** | *read* | *s* | Informational path of the loaded file descriptor, when available. Empty for JSON profiles, startup, or a controller fault. |

### Methods

#### LoadProfile

Load a YAML profile from a caller-opened readable regular file, including arbitrary paths and symlinks. Pipes, sockets, devices, directories and unreadable descriptors are rejected. Input is limited to 64 KiB and copied without reopening the path or changing the shared offset. The complete profile is validated before changing any fan.

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **file** | *in* | *h* |  |

#### LoadProfileJson

Load a profile as a JSON string, limited to 64 KiB. The complete profile is validated before changing any fan. Clients must handle saving and reloading the profile after service startup.

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **json** | *in* | *s* |  |

#### EnumerateFans

Return all discovered fans for this device.

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **** | *out* | *ao* |  |

#### EnumerateSensors

Return all discovered temperature sensors for this device.

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **** | *out* | *ao* |  |

### Signals
