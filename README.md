<h1 align="center">
  <img src="https://raw.githubusercontent.com/ShadowBlip/PowerStation/main/icon.svg" alt="PowerStation Logo" width="200">
  <br>
  PowerStation
</h1>

<p align="center">
  <a href="https://github.com/ShadowBlip/PowerStation/stargazers"><img src="https://img.shields.io/github/stars/ShadowBlip/PowerStation" /></a>
  <a href="https://github.com/ShadowBlip/PowerStation/blob/main/LICENSE"><img src="https://img.shields.io/github/license/ShadowBlip/PowerStation" /></a>
  <a href="https://discord.gg/fKsUbrt"><img src="https://img.shields.io/badge/discord-server-%235865F2" /></a>
  <br>
</p>

## About

PowerStation is an open source TDP control and performance daemon for Linux that 
can be used to control CPU and GPU settings for better performance and battery
life. Performance control is done through [DBus](https://www.freedesktop.org/wiki/Software/dbus/)
to provide a UI-agnostic interface to CPU and GPU settings.

## Install

You can install with:

```bash
make build
sudo make install
```

If you are using ArchLinux, you can install PowerStation from the AUR:

```bash
yay -S powerstation-bin
```

Then start the service with:

```bash
sudo systemctl enable powerstation
sudo systemctl start powerstation
```

## Documentation

XML specifications for all interfaces can be found in [bindings/dbus-xml](./bindings/dbus-xml).

Individual interface documentation can be found here:

* [org.shadowblip.CPU](./docs/cpu.md)
* [org.shadowblip.CPU.Core](./docs/cpu-core.md)
* [org.shadowblip.GPU](./docs/gpu.md)
* [org.shadowblip.GPU.Card](./docs/gpu-card.md)
* [org.shadowblip.GPU.Card.Connector](./docs/gpu-card-connector.md)

## Usage

When PowerStation is running as a service, you can interact with it over DBus.
There are various DBus libraries available for popular programming languages
like Python, Rust, C++, etc.

You can also interface with DBus using the `busctl` command:

```bash
busctl tree org.shadowblip.PowerStation
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
busctl introspect org.shadowblip.PowerStation /org/shadowblip/Performance/GPU/Card2
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
org.shadowblip.GPU.Card             interface -         -                      -
.EnumerateConnectors                method    -         ao                     -
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
org.shadowblip.GPU.Card.TDP         interface -         -                      -
.Boost                              property  d         11                     emits-change writable
.PowerProfile                       property  s         "max-performance"      emits-change writable
.TDP                                property  d         55                     emits-change writable
.ThermalThrottleLimitC              property  d         95                     emits-change writable
```

## Testing

When PowerStation is running, you can test setting properties with:

```bash
busctl set-property org.shadowblip.PowerStation /org/shadowblip/Performance/CPU/Core11 org.shadowblip.CPU.Core Online "b" False
```


## License

PowerStation is licensed under THE GNU GPLv3+. See LICENSE for details.
