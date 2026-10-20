use crate::hal::pac::tc1::tccr1b;
use avr_device::generic::W;

pub const PRESCALAR: u64 = cfg_select! {
    feature = "prescalar1" => 1,
    feature = "prescalar8" => 8,
    feature = "prescalar64" => 64,
    feature = "prescalar256" => 256,
    feature = "prescalar1024" => 1024,
    _ => 256,
};

pub fn set_prescalar(reg: &mut tccr1b::W) -> &mut W<tccr1b::TCCR1B_SPEC> {
    cfg_select! {
        feature = "prescalar1" => reg.cs1().direct(),
        feature = "prescalar8" => reg.cs1().prescale_8(),
        feature = "prescalar64" => reg.cs1().prescale_64(),
        feature = "prescalar256" => reg.cs1().prescale_256(),
        feature = "prescalar1024" => reg.cs1().prescale_1024(),
        _ => reg.cs1().prescale_256(),
    }
}
