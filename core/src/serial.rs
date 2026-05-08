pub const SB_REGISTER: u16 = 0xFF01;
pub const SC_REGISTER: u16 = 0xFF02;
const SC_TRANSFER_START_BIT: u8 = 0x80;
const SC_INTERNAL_CLOCK_BIT: u8 = 0x01;

/// Link-cable peer model used when completing serial transfers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SerialConnectionMode {
    /// No peer is attached; completed transfers read back idle high bits.
    #[default]
    Disconnected,
    /// A local test peer echoes each transmitted byte back into SB.
    LocalLoopback,
}

/// Minimal DMG serial-port model used for ROM pass/fail diagnostics.
///
/// This implementation keeps SB/SC register semantics and provides a basic
/// internal-clock transfer completion path. Networked link-cable behavior
/// remains out of scope for now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SerialPort {
    sb: u8,
    sc: u8,
    connection_mode: SerialConnectionMode,
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

    pub const fn connection_mode(&self) -> SerialConnectionMode {
        self.connection_mode
    }

    pub fn set_connection_mode(&mut self, connection_mode: SerialConnectionMode) {
        self.connection_mode = connection_mode;
    }

    pub fn take_transfer_log(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transfer_log)
    }

    fn maybe_complete_internal_transfer(&mut self) {
        if (self.sc & (SC_TRANSFER_START_BIT | SC_INTERNAL_CLOCK_BIT))
            == (SC_TRANSFER_START_BIT | SC_INTERNAL_CLOCK_BIT)
        {
            let transmitted = self.sb;
            self.transfer_log.push(transmitted);
            self.sb = self.received_byte_for_transfer(transmitted);
            self.sc &= !SC_TRANSFER_START_BIT;
        }
    }

    const fn received_byte_for_transfer(&self, transmitted: u8) -> u8 {
        match self.connection_mode {
            SerialConnectionMode::Disconnected => 0xFF,
            SerialConnectionMode::LocalLoopback => transmitted,
        }
    }
}
