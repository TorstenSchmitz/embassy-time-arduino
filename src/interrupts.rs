use super::tc1_compa;
use super::tc1_ovf;

// Ugly hack to set interrupt handler on every configuration
#[cfg_attr(
    any(
        feature = "arduino-uno",
        feature = "trinket-pro",
        feature = "sparkfun-promini-3v3",
        feature = "sparkfun-promini-5v",
    ),
    avr_device::interrupt(atmega328p)
)]
#[cfg_attr(
    any(feature = "nano168", feature = "arduino-diecimila"),
    avr_device::interrupt(atmega168)
)]
#[cfg_attr(
    any(
        feature = "arduino-leonardo",
        feature = "arduino-micro",
        feature = "sparkfun-promicro"
    ),
    avr_device::interrupt(atmega32u4)
)]
#[cfg_attr(any(feature = "arduino-mega1280"), avr_device::interrupt(atmega1280))]
#[cfg_attr(any(feature = "arduino-mega2560"), avr_device::interrupt(atmega2560))]
fn TIMER1_OVF() {
    tc1_ovf();
}

#[cfg_attr(
    any(
        feature = "arduino-uno",
        feature = "trinket-pro",
        feature = "sparkfun-promini-3v3",
        feature = "sparkfun-promini-5v",
    ),
    avr_device::interrupt(atmega328p)
)]
#[cfg_attr(
    any(feature = "nano168", feature = "arduino-diecimila"),
    avr_device::interrupt(atmega168)
)]
#[cfg_attr(
    any(
        feature = "arduino-leonardo",
        feature = "arduino-micro",
        feature = "sparkfun-promicro"
    ),
    avr_device::interrupt(atmega32u4)
)]
#[cfg_attr(any(feature = "arduino-mega1280"), avr_device::interrupt(atmega1280))]
#[cfg_attr(any(feature = "arduino-mega2560"), avr_device::interrupt(atmega2560))]
fn TIMER1_COMPA() {
    tc1_compa();
}
