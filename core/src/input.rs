#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoypadButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Joypad {
    p1_select: u8,
    pressed: u8,
}

impl Default for Joypad {
    fn default() -> Self {
        Self {
            // No row selected (active-low), no buttons pressed.
            p1_select: 0x30,
            pressed: 0,
        }
    }
}

impl Joypad {
    const P1_SELECT_MASK: u8 = 0x30;

    pub fn write_p1(&mut self, value: u8) {
        self.p1_select = value & Self::P1_SELECT_MASK;
    }

    pub fn read_p1(&self) -> u8 {
        let mut low_nibble = 0x0F;
        if self.selects_dpad_row() {
            low_nibble &= !((self.pressed >> 4) & 0x0F);
        }
        if self.selects_button_row() {
            low_nibble &= !(self.pressed & 0x0F);
        }
        0xC0 | self.p1_select | low_nibble
    }

    pub fn set_button_pressed(&mut self, button: JoypadButton, pressed: bool) {
        let bit = button.bit();
        if pressed {
            self.pressed |= 1 << bit;
        } else {
            self.pressed &= !(1 << bit);
        }
    }

    fn selects_dpad_row(&self) -> bool {
        self.p1_select & 0x10 == 0
    }

    fn selects_button_row(&self) -> bool {
        self.p1_select & 0x20 == 0
    }
}

impl JoypadButton {
    const fn bit(self) -> u8 {
        match self {
            Self::A => 0,
            Self::B => 1,
            Self::Select => 2,
            Self::Start => 3,
            Self::Right => 4,
            Self::Left => 5,
            Self::Up => 6,
            Self::Down => 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Joypad, JoypadButton};

    #[test]
    fn p1_read_reflects_button_row_selection() {
        let mut joypad = Joypad::default();
        joypad.set_button_pressed(JoypadButton::A, true);
        joypad.set_button_pressed(JoypadButton::Start, true);
        joypad.write_p1(0x10);

        assert_eq!(joypad.read_p1() & 0x0F, 0b0110);
    }

    #[test]
    fn p1_read_reflects_direction_row_selection() {
        let mut joypad = Joypad::default();
        joypad.set_button_pressed(JoypadButton::Right, true);
        joypad.set_button_pressed(JoypadButton::Up, true);
        joypad.write_p1(0x20);

        assert_eq!(joypad.read_p1() & 0x0F, 0b1010);
    }
}
