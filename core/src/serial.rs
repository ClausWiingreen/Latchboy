use std::collections::VecDeque;

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
    /// A remotely driven peer supplies bytes from an inbound receive queue.
    NetworkedPeer,
}

/// Minimal DMG serial-port model used for ROM pass/fail diagnostics.
///
/// This implementation keeps SB/SC register semantics and provides a basic
/// internal-clock transfer completion path. Platform transports can opt into
/// [`SerialConnectionMode::NetworkedPeer`], drain [`SerialPort::take_transfer_log`]
/// for outbound bytes, and enqueue bytes received from a remote peer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SerialPort {
    sb: u8,
    sc: u8,
    connection_mode: SerialConnectionMode,
    transfer_log: Vec<u8>,
    peer_receive_queue: VecDeque<u8>,
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
        if self.connection_mode != connection_mode {
            self.peer_receive_queue.clear();
        }
        self.connection_mode = connection_mode;
    }

    pub fn take_transfer_log(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transfer_log)
    }

    /// Queues a byte received from a networked link-cable peer.
    ///
    /// Queued bytes are consumed one at a time by completed transfers while
    /// [`SerialConnectionMode::NetworkedPeer`] is selected. Queuing bytes in
    /// other modes is harmless, but switching modes clears any pending bytes
    /// to avoid carrying stale remote input across peer changes.
    pub fn enqueue_peer_received_byte(&mut self, value: u8) {
        self.peer_receive_queue.push_back(value);
    }

    /// Queues bytes received from a networked link-cable peer in FIFO order.
    pub fn enqueue_peer_received_bytes(&mut self, values: impl IntoIterator<Item = u8>) {
        self.peer_receive_queue.extend(values);
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

    fn received_byte_for_transfer(&mut self, transmitted: u8) -> u8 {
        match self.connection_mode {
            SerialConnectionMode::Disconnected => 0xFF,
            SerialConnectionMode::LocalLoopback => transmitted,
            SerialConnectionMode::NetworkedPeer => {
                self.peer_receive_queue.pop_front().unwrap_or(0xFF)
            }
        }
    }
}
