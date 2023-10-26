# Lightning Bus

Open source performance daemon with DBus interface

## Install

You can install with:

```bash
make build
sudo make install
```

## Testing

When running, you can test setting properties with:

```bash
busctl set-property org.shadowblip.LightningBus /org/shadowblip/Performance/CPU/Core11 org.shadowblip.CPU.Core Online "b" False
```

## References

https://nyirog.medium.com/register-dbus-service-f923dfca9f1
