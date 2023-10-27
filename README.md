# Lightning Bus

Open source performance daemon with DBus interface

## Install

You can install with:

```bash
make build
sudo make install
```

## Usage

When LightningBus is running as a service, you can interact with it over DBus.
There are various DBus libraries available for popular programming languages
like Python, Rust, C++, etc.

You can also interface with DBus using the `busctl` command:

```bash
busctl tree org.shadowblip.LightningBus
```

```bash
└─ /org
  └─ /org/shadowblip
    └─ /org/shadowblip/Performance
      ├─ /org/shadowblip/Performance/CPU
      │ ├─ /org/shadowblip/Performance/CPU/Core0
      │ ├─ /org/shadowblip/Performance/CPU/Core1
      │ ├─ /org/shadowblip/Performance/CPU/Core10
      │ ├─ /org/shadowblip/Performance/CPU/Core11
      │ ├─ /org/shadowblip/Performance/CPU/Core2
      │ ├─ /org/shadowblip/Performance/CPU/Core3
      │ ├─ /org/shadowblip/Performance/CPU/Core4
      │ ├─ /org/shadowblip/Performance/CPU/Core5
      │ ├─ /org/shadowblip/Performance/CPU/Core6
      │ ├─ /org/shadowblip/Performance/CPU/Core7
      │ ├─ /org/shadowblip/Performance/CPU/Core8
      │ └─ /org/shadowblip/Performance/CPU/Core9
      └─ /org/shadowblip/Performance/GPU
        ├─ /org/shadowblip/Performance/GPU/Card1
        │ └─ /org/shadowblip/Performance/GPU/Card1/HDMI
        │   └─ /org/shadowblip/Performance/GPU/Card1/HDMI/A
        │     └─ /org/shadowblip/Performance/GPU/Card1/HDMI/A/1
        └─ /org/shadowblip/Performance/GPU/Card2
          └─ /org/shadowblip/Performance/GPU/Card2/eDP
            └─ /org/shadowblip/Performance/GPU/Card2/eDP/1
```

```bash
busctl introspect org.shadowblip.LightningBus /org/shadowblip/Performance/GPU/Card2
```

```bash
NAME                                TYPE      SIGNATURE RESULT/VALUE           FLAGS
org.freedesktop.DBus.Introspectable interface -         -                      -
.Introspect                         method    -         s                      -
org.freedesktop.DBus.Peer           interface -         -                      -
.GetMachineId                       method    -         s                      -
.Ping                               method    -         -                      -
org.freedesktop.DBus.Properties     interface -         -                      -
.Get                                method    ss        v                      -
.GetAll                             method    s         a{sv}                  -
.Set                                method    ssv       -                      -
.PropertiesChanged                  signal    sa{sv}as  -                      -
org.shadowblip.GPU                  interface -         -                      -
.Class                              property  s         "integrated"           emits-change
.ClassId                            property  s         "030000"               emits-change
.ClockLimitMhzMax                   property  d         -                      emits-change
.ClockLimitMhzMin                   property  d         -                      emits-change
.ClockValueMhzMax                   property  d         -                      emits-change writable
.ClockValueMhzMin                   property  d         -                      emits-change writable
.Device                             property  s         "Renoir"               emits-change
.DeviceId                           property  s         "1636"                 emits-change
.ManualClock                        property  b         false                  emits-change writable
.Name                               property  s         "card2"                emits-change
.Path                               property  s         "/sys/class/drm/card2" emits-change
.RevisionId                         property  s         "c7"                   emits-change
.Subdevice                          property  s         ""                     emits-change
.SubdeviceId                        property  s         "12b5"                 emits-change
.SubvendorId                        property  s         "1462"                 emits-change
.Vendor                             property  s         "AMD"                  emits-change
.VendorId                           property  s         "1002"                 emits-change
```

## Testing

When running, you can test setting properties with:

```bash
busctl set-property org.shadowblip.LightningBus /org/shadowblip/Performance/CPU/Core11 org.shadowblip.CPU.Core Online "b" False
```

## References

https://nyirog.medium.com/register-dbus-service-f923dfca9f1
