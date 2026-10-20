#![no_std]
#![feature(const_trait_impl)]
#![feature(const_result_trait_fn)]
#![feature(const_option_ops)]
#![feature(abi_avr_interrupt)]
extern crate static_assertions;
mod interrupts;
mod prescalar;

pub use arduino_hal as hal;
use core::cell::Cell;
use core::task::Waker;
use critical_section::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time_driver::Driver;
use hal::pac::TC1;

use crate::prescalar::PRESCALAR;

pub static TICKS_LAST_OVF: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));
static LAST_TICKS_ELAPSED: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));

#[cfg(not(any(feature = "hal-selected", feature = "_library_build_check")))]
compile_error!("select the board via crate features please");
/// embassy_time implementation for arduinos
/// If no prescaling is configure 256 will be used.
/// higher prescaling results in fewer interrupts,
/// but lower time resolution.
/// EMBASSY_TIME_QUEUE_SIZE controls the size of the queue, default is 8.
/// Set feature embassy-time-driver/tick-hz* if somewhat precise timing is needed.
/// You must select your board as feature flag
/// and you _must_ to initialize timer before using embassy_time
/// ```rust
/// init_system_time(&mut dp.TC1);
/// ```
pub struct EmbassyTimeDriverArduino {
    next_alarm: Mutex<Cell<u64>>,
}
use embassy_sync::priority_channel::Min;
use embassy_sync::priority_channel::PriorityChannel;

#[derive(Clone)]
struct Task {
    at: u64,
    waker: Waker,
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at && self.waker.will_wake(&other.waker)
    }
}
impl Eq for Task {}
impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.at.cmp(&other.at))
    }
}
impl Ord for Task {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.at.cmp(&other.at)
    }
}

const QUEUE_SIZE: usize =
    usize::from_str_radix(option_env!("EMBASSY_TIME_QUEUE_SIZE").unwrap_or("8"), 10).unwrap_or(8);

const QUEUE: PriorityChannel<CriticalSectionRawMutex, Task, Min, QUEUE_SIZE> =
    PriorityChannel::new();

// ensure tc1 is indeed a ZST
static_assertions::assert_eq_size!(TC1, ());
macro_rules! tc1 {
    () => {
        // SAFETY: TC1 is ZST
        unsafe { core::mem::MaybeUninit::<TC1>::uninit().assume_init() }
    };
}

const MIN_TICK_TO_INTTERUPT: u16 = 200;

impl EmbassyTimeDriverArduino {
    const fn new() -> Self {
        Self {
            next_alarm: Mutex::new(Cell::new(0)),
        }
    }

    #[inline(never)]
    fn time_now(&self) -> u64 {
        critical_section::with(|cs| {
            let last_ticks_elapsed = LAST_TICKS_ELAPSED.borrow(cs);
            let mut ticks_last_ovf = TICKS_LAST_OVF.borrow(cs).get();
            let tcnt = tc1!().tcnt1().read().bits();
            // Overflow interrupt pending
            if tcnt < u16::MAX / 4 && tc1!().tifr1().read().tov1().bit() {
                ticks_last_ovf += u16::MAX as u64;
            }
            if last_ticks_elapsed.get() > ticks_last_ovf + tcnt as u64 {
                ticks_last_ovf += u16::MAX as u64;
            }
            last_ticks_elapsed.replace(ticks_last_ovf + tcnt as u64);
            last_ticks_elapsed.get()
        })
    }
    /// update compA for next wake
    fn update_ocr1a(&self, cs: critical_section::CriticalSection) {
        let at = self.next_alarm.borrow(cs).get();
        let now = self.time_now();
        if at <= now {
            core::hint::cold_path();
            return;
        };
        let next_alarm = core::cmp::max((at - now) as u16, MIN_TICK_TO_INTTERUPT);
        let tcnt: u16 = tc1!().tcnt1().read().bits();
        let next_ovf = u16::MAX - tcnt;
        if next_alarm >= next_ovf {
            return;
        }
        #[cfg(feature = "defmt")]
        defmt::trace!("alarm set");
        tc1!().ocr1a().write(|w| w.set(tcnt + next_alarm));
        tc1!().timsk1().write(|w| w.ocie1a().set_bit());
    }

    fn set_alarm(&self, cs: critical_section::CriticalSection, at: u64) {
        let now = self.time_now();
        if at <= now {
            return;
        }
        self.next_alarm
            .borrow(cs)
            .update(|old| core::cmp::min(old, at));
        self.update_ocr1a(cs);
    }
    fn scheduler(&self, cs: critical_section::CriticalSection) {
        self.wake_all();
        if let Ok(next) = QUEUE.try_peek() {
            self.set_alarm(cs, next.at);
        } else {
            defmt::warn!("scheduler queue empty");
        }
    }

    fn wake_one(&self) -> bool {
        while let Ok(task) = QUEUE.try_peek() {
            if task.at < self.time_now()
                && QUEUE.try_receive().and_then(|t| Ok(t.waker.wake())).is_ok()
            {
                return true;
            }
        }
        false
    }
    fn wake_all(&self) -> u8 {
        let mut count = 0;
        while let Ok(task) = QUEUE.try_peek() {
            if task.at < self.time_now() {
                match QUEUE.try_receive().and_then(|t| Ok(t.waker.wake())) {
                    Ok(_) => count += 1,
                    _ => (),
                }
            }
        }
        count
    }
}
impl Driver for EmbassyTimeDriverArduino {
    #[inline(always)]
    fn now(&self) -> u64 {
        self.time_now() * PRESCALAR
    }

    #[inline(never)]
    fn schedule_wake(&self, at: u64, waker: &Waker) {
        let at = at / PRESCALAR + 1;
        while let Err(_) = QUEUE.try_send(Task {
            at,
            waker: waker.clone(),
        }) {
            if !self.wake_one() {
                defmt::error!("timer queue full");
                break;
            }
        }
    }
}

pub fn init_system_time(tc: &mut TC1) {
    critical_section::with(|cs| {
        TICKS_LAST_OVF.borrow(cs).set(0);
        LAST_TICKS_ELAPSED.borrow(cs).set(0);
        tc.tccr1b().write(prescalar::set_prescalar);
        tc.tccr1a().write(|w| w.com1a().disconnected());
        // enable overflow interrupts
        tc.timsk1()
            .write(|w| w.toie1().set_bit().ocie1a().set_bit());
        tc.tcnt1().reset();
    });
}

#[inline(always)]
pub fn tc1_ovf() {
    critical_section::with(|cs| {
        TICKS_LAST_OVF.borrow(cs).update(|x| x + u16::MAX as u64);
        LAST_TICKS_ELAPSED
            .borrow(cs)
            .update(|last| core::cmp::max(last, TICKS_LAST_OVF.borrow(cs).get()));
        // clear ovf flag
        tc1!().tifr1().write(|w| w.tov1().set_bit());
        DRIVER.scheduler(cs);
    })
}

#[inline(always)]
pub fn tc1_compa() {
    critical_section::with(|cs| {
        tc1!().timsk1().write(|w| w.ocie1a().clear_bit());
        DRIVER.scheduler(cs);
    })
}

embassy_time_driver::time_driver_impl!(static DRIVER: EmbassyTimeDriverArduino = EmbassyTimeDriverArduino::new());
