#![no_std]
#![feature(const_trait_impl)]
#![feature(const_result_trait_fn)]
#![feature(const_option_ops)]
#![feature(abi_avr_interrupt)]
extern crate static_assertions;
mod interrupts;
mod prescalar;

pub use arduino_hal as hal;
use core::cell::{Cell, RefCell};
use core::task::Waker;
use critical_section::Mutex;
use embassy_time_driver::Driver;
use embassy_time_queue_utils::queue_generic::ConstGenericQueue;
use hal::pac::TC1;

use crate::prescalar::PRESCALAR;

pub static TICKS_ELAPSED: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));

#[cfg(not(feature = "hal-selected"))]
compile_error!("select the board via crate features please");
/// embassy_time implementation for arduinos
/// If no prescaling is configure 256 will be used.
/// higher prescaling results in fewer interrupts,
/// but lower time resolution.
/// TODO: adjust tick-hz according to board specs.
/// EMBASSY_TIME_QUEUE_SIZE controls the size of the queue, default is 4.
///
/// You must select your board as feature flag
/// and you _must_ to initialize timer before using embassy_time
/// ```rust
/// init_system_time(&mut dp.TC1);
/// ```
pub struct EmbassyTimeDriverArduino {
    next_alarm: Mutex<Cell<u64>>,
}

const QUEUE_SIZE: usize =
    usize::from_str_radix(option_env!("EMBASSY_TIME_QUEUE_SIZE").unwrap_or("4"), 10).unwrap_or(4);

pub static QUEUE: Mutex<RefCell<ConstGenericQueue<QUEUE_SIZE>>> =
    Mutex::new(RefCell::new(ConstGenericQueue::new()));

// ensure tc1 is indeed a ZST
static_assertions::assert_eq_size!(TC1, ());
macro_rules! tc1 {
    () => {
        // SAFETY: TC1 is ZST
        unsafe { core::mem::MaybeUninit::<TC1>::uninit().assume_init() }
    };
}
impl EmbassyTimeDriverArduino {
    const fn new() -> Self {
        Self {
            next_alarm: Mutex::new(Cell::new(0)),
        }
    }

    #[inline(never)]
    fn time_now(&self) -> u64 {
        let tcnt = tc1!().tcnt1().read().bits();
        critical_section::with(|cs| {
            let ticks_elapsed = TICKS_ELAPSED.borrow(cs);
            ticks_elapsed.update(|x| x + tcnt as u64);
            // Overflow interrupt pending
            if tc1!().tifr1().read().tov1().bit() {
                ticks_elapsed.update(|x| x + u16::MAX as u64);
            }
            ticks_elapsed.get()
        })
    }
    fn update_ocr1a(&self, cs: critical_section::CriticalSection) {
        let next_ovf = self.time_now().next_multiple_of(u16::MAX as u64);
        let next_alarm = self.next_alarm.borrow(cs).get();
        if next_alarm < next_ovf {
            tc1!().ocr1a().write(|w| w.set(next_alarm as u16));
        }
    }

    fn set_alarm(&self, cs: critical_section::CriticalSection, at: u64) -> bool {
        if self.now() >= at {
            return false;
        }
        self.next_alarm
            .borrow(cs)
            .update(|old| core::cmp::min(old, at));
        self.update_ocr1a(cs);
        true
    }
    fn scheduler(&self, cs: critical_section::CriticalSection) {
        let mut queue = QUEUE.borrow_ref_mut(cs);
        let mut next = queue.next_expiration(self.time_now());
        while !self.set_alarm(cs, next) {
            next = queue.next_expiration(self.time_now());
        }
    }
}
impl Driver for EmbassyTimeDriverArduino {
    #[inline(always)]
    fn now(&self) -> u64 {
        self.time_now()
    }

    #[inline(never)]
    fn schedule_wake(&self, at: u64, waker: &Waker) {
        critical_section::with(|cs| {
            let mut queue = QUEUE.borrow_ref_mut(cs);
            if queue.schedule_wake(at, waker) {
                drop(queue);
                self.scheduler(cs);
            }
        });
    }
}

static LAST_TICKS_ELAPSED: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));

pub fn init_system_time(tc: &mut TC1) {
    critical_section::with(|cs| {
        TICKS_ELAPSED.borrow(cs).set(0);
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
        TICKS_ELAPSED.borrow(cs).update(|x| x + u16::MAX as u64);
        DRIVER.scheduler(cs);
    })
}

#[inline(always)]
pub fn tc1_compa() {
    critical_section::with(|cs| {
        DRIVER.scheduler(cs);
    })
}

embassy_time_driver::time_driver_impl!(static DRIVER: EmbassyTimeDriverArduino = EmbassyTimeDriverArduino::new());
