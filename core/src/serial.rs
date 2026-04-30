pub const SB_REGISTER: u16 = 0xFF01;
pub const SC_REGISTER: u16 = 0xFF02;
const SC_TRANSFER_START_BIT: u8 = 0x80;
const SC_INTERNAL_CLOCK_BIT: u8 = 0x01;

/// Minimal DMG serial-port model used for ROM pass/fail diagnostics.
///
/// This implementation keeps SB/SC register semantics and provides a basic
/// internal-clock transfer completion path. Full link-cable behavior remains
/// out of scope for now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SerialPort {
    sb: u8,
    sc: u8,
    transfer_log: Vec<u8>,
}

impl SerialPort {
    pub fn read(&self, address: u16) -> Option<u8> {
        match address {
            SB_REGISTER => Some(self.sb),
            SC_REGISTER => Some(self.sc | 0x7E),
            _ => None,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) -> bool {
        match address {
            SB_REGISTER => {
                self.sb = value;
                true
            }
            SC_REGISTER => {
                self.sc = value & 0x81;
                self.maybe_complete_internal_transfer();
                true
            }
            _ => false,
        }
    }

    pub fn take_transfer_log(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transfer_log)
    }

    fn maybe_complete_internal_transfer(&mut self) {
        if (self.sc & (SC_TRANSFER_START_BIT | SC_INTERNAL_CLOCK_BIT))
            == (SC_TRANSFER_START_BIT | SC_INTERNAL_CLOCK_BIT)
        {
            self.transfer_log.push(self.sb);
            self.sb = 0xFF;
            self.sc &= !SC_TRANSFER_START_BIT;
        }
    }
}
