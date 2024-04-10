//! # DBus interface proxy for: `org.asuslinux.Daemon`
//!
//! This code was generated by `zbus-xmlgen` `3.1.1` from DBus introspection data.
//! Source: `org-asuslinux-platform-4.xml`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://dbus.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::PropertiesProxy`]
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PeerProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.asuslinux.Daemon", assume_defaults = true)]
trait Daemon {
    /// NextThrottleThermalPolicy method
    fn next_throttle_thermal_policy(&self) -> zbus::Result<()>;

    /// SupportedInterfaces method
    fn supported_interfaces(&self) -> zbus::Result<Vec<String>>;

    /// SupportedProperties method
    fn supported_properties(&self) -> zbus::Result<Vec<String>>;

    /// ChargeControlEndThreshold property
    #[dbus_proxy(property)]
    fn charge_control_end_threshold(&self) -> zbus::Result<u8>;
    fn set_charge_control_end_threshold(&self, value: u8) -> zbus::Result<()>;

    /// DgpuDisable property
    #[dbus_proxy(property)]
    fn dgpu_disable(&self) -> zbus::Result<bool>;

    /// EgpuEnable property
    #[dbus_proxy(property)]
    fn egpu_enable(&self) -> zbus::Result<bool>;

    /// GpuMuxMode property
    #[dbus_proxy(property)]
    fn gpu_mux_mode(&self) -> zbus::Result<u8>;
    fn set_gpu_mux_mode(&self, value: u8) -> zbus::Result<()>;

    /// MiniLedMode property
    #[dbus_proxy(property)]
    fn mini_led_mode(&self) -> zbus::Result<bool>;
    fn set_mini_led_mode(&self, value: bool) -> zbus::Result<()>;

    /// NvDynamicBoost property
    #[dbus_proxy(property)]
    fn nv_dynamic_boost(&self) -> zbus::Result<u8>;
    fn set_nv_dynamic_boost(&self, value: u8) -> zbus::Result<()>;

    /// NvTempTarget property
    #[dbus_proxy(property)]
    fn nv_temp_target(&self) -> zbus::Result<u8>;
    fn set_nv_temp_target(&self, value: u8) -> zbus::Result<()>;

    /// PanelOd property
    #[dbus_proxy(property)]
    fn panel_od(&self) -> zbus::Result<bool>;
    fn set_panel_od(&self, value: bool) -> zbus::Result<()>;

    /// PostAnimationSound property
    #[dbus_proxy(property)]
    fn post_animation_sound(&self) -> zbus::Result<bool>;
    fn set_post_animation_sound(&self, value: bool) -> zbus::Result<()>;

    /// PptApuSppt property
    #[dbus_proxy(property)]
    fn ppt_apu_sppt(&self) -> zbus::Result<u8>;
    fn set_ppt_apu_sppt(&self, value: u8) -> zbus::Result<()>;

    /// PptFppt property
    #[dbus_proxy(property)]
    fn ppt_fppt(&self) -> zbus::Result<u8>;
    fn set_ppt_fppt(&self, value: u8) -> zbus::Result<()>;

    /// PptPl1Spl property
    #[dbus_proxy(property)]
    fn ppt_pl1_spl(&self) -> zbus::Result<u8>;
    fn set_ppt_pl1_spl(&self, value: u8) -> zbus::Result<()>;

    /// PptPl2Sppt property
    #[dbus_proxy(property)]
    fn ppt_pl2_sppt(&self) -> zbus::Result<u8>;
    fn set_ppt_pl2_sppt(&self, value: u8) -> zbus::Result<()>;

    /// PptPlatformSppt property
    #[dbus_proxy(property)]
    fn ppt_platform_sppt(&self) -> zbus::Result<u8>;
    fn set_ppt_platform_sppt(&self, value: u8) -> zbus::Result<()>;

    /// ThrottleBalancedEpp property
    #[dbus_proxy(property)]
    fn throttle_balanced_epp(&self) -> zbus::Result<u32>;
    fn set_throttle_balanced_epp(&self, value: u32) -> zbus::Result<()>;

    /// ThrottlePerformanceEpp property
    #[dbus_proxy(property)]
    fn throttle_performance_epp(&self) -> zbus::Result<u32>;
    fn set_throttle_performance_epp(&self, value: u32) -> zbus::Result<()>;

    /// ThrottlePolicyLinkedEpp property
    #[dbus_proxy(property)]
    fn throttle_policy_linked_epp(&self) -> zbus::Result<bool>;
    fn set_throttle_policy_linked_epp(&self, value: bool) -> zbus::Result<()>;

    /// ThrottlePolicyOnAc property
    #[dbus_proxy(property)]
    fn throttle_policy_on_ac(&self) -> zbus::Result<u32>;
    fn set_throttle_policy_on_ac(&self, value: u32) -> zbus::Result<()>;

    /// ThrottlePolicyOnBattery property
    #[dbus_proxy(property)]
    fn throttle_policy_on_battery(&self) -> zbus::Result<u32>;
    fn set_throttle_policy_on_battery(&self, value: u32) -> zbus::Result<()>;

    /// ThrottleQuietEpp property
    #[dbus_proxy(property)]
    fn throttle_quiet_epp(&self) -> zbus::Result<u32>;
    fn set_throttle_quiet_epp(&self, value: u32) -> zbus::Result<()>;

    /// ThrottleThermalPolicy property
    #[dbus_proxy(property)]
    fn throttle_thermal_policy(&self) -> zbus::Result<u32>;
    fn set_throttle_thermal_policy(&self, value: u32) -> zbus::Result<()>;
}
