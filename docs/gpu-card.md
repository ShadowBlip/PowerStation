# GPU.Card DBus Interface API

## org.shadowblip.GPU.Card.TDP

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **Boost** | *readwrite* | *d* |  |
| **PowerProfile** | *readwrite* | *s* |  |
| **TDP** | *readwrite* | *d* |  |
| **ThermalThrottleLimitC** | *readwrite* | *d* |  |

### Methods

### Signals

## org.shadowblip.GPU.Card

### Properties

| Name | Access | Type | Description |
| --- | :---: | :---: | --- |
| **Class** | *read* | *s* |  |
| **ClassId** | *read* | *s* |  |
| **ClockLimitMhzMax** | *read* | *d* |  |
| **ClockLimitMhzMin** | *read* | *d* |  |
| **ClockValueMhzMax** | *readwrite* | *d* |  |
| **ClockValueMhzMin** | *readwrite* | *d* |  |
| **Device** | *read* | *s* |  |
| **DeviceId** | *read* | *s* |  |
| **ManualClock** | *readwrite* | *b* |  |
| **Name** | *read* | *s* |  |
| **Path** | *read* | *s* |  |
| **RevisionId** | *read* | *s* |  |
| **Subdevice** | *read* | *s* |  |
| **SubdeviceId** | *read* | *s* |  |
| **SubvendorId** | *read* | *s* |  |
| **Vendor** | *read* | *s* |  |
| **VendorId** | *read* | *s* |  |

### Methods

#### EnumerateConnectors

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| \*\*\*\* | *out* | *ao* |  |

### Signals

## org.freedesktop.DBus.Properties

### Methods

#### Get

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* |  |
| **property_name** | *in* | *s* |  |
| \*\*\*\* | *out* | *v* |  |

#### Set

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* |  |
| **property_name** | *in* | *s* |  |
| **value** | *in* | *v* |  |

#### GetAll

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | *in* | *s* |  |
| \*\*\*\* | *out* | *a{sv}* |  |

### Signals

#### PropertiesChanged

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| **interface_name** | \*\* | *s* |  |
| **changed_properties** | \*\* | *a{sv}* |  |
| **invalidated_properties** | \*\* | *as* |  |

## org.freedesktop.DBus.Peer

### Methods

#### Ping

#### GetMachineId

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| \*\*\*\* | *out* | *s* |  |

### Signals

## org.freedesktop.DBus.Introspectable

### Methods

#### Introspect

##### Arguments

| Name | Direction | Type | Description |
| --- | :---: | :---: | --- |
| \*\*\*\* | *out* | *s* |  |

### Signals
