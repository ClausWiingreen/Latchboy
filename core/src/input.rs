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

    pub fn write_p1(&mut self, value: u8) -> bool {
        let previous_low_nibble = self.poll_low_nibble();
        self.p1_select = value & Self::P1_SELECT_MASK;
        Self::has_falling_edge(previous_low_nibble, self.poll_low_nibble())
    }

    pub fn read_p1(&self) -> u8 {
        0xC0 | self.p1_select | self.poll_low_nibble()
    }

    pub fn set_button_pressed(&mut self, button: JoypadButton, pressed: bool) -> bool {
        let previous_low_nibble = self.poll_low_nibble();
        let bit = button.bit();
        if pressed {
            self.pressed |= 1 << bit;
        } else {
            self.pressed &= !(1 << bit);
        }
        Self::has_falling_edge(previous_low_nibble, self.poll_low_nibble())
    }

    fn selects_dpad_row(&self) -> bool {
        self.p1_select & 0x10 == 0
    }

    fn selects_button_row(&self) -> bool {
        self.p1_select & 0x20 == 0
    }

    fn poll_low_nibble(&self) -> u8 {
        let mut low_nibble = 0x0F;
        if self.selects_dpad_row() {
            low_nibble &= !((self.pressed >> 4) & 0x0F);
        }
        if self.selects_button_row() {
            low_nibble &= !(self.pressed & 0x0F);
        }
        low_nibble
    }

    const fn has_falling_edge(previous_low_nibble: u8, current_low_nibble: u8) -> bool {
        (previous_low_nibble & !current_low_nibble) != 0
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

    #[test]
    fn button_press_returns_interrupt_request_on_selected_row_falling_edge() {
        let mut joypad = Joypad::default();
        joypad.write_p1(0x10);

        assert!(joypad.set_button_pressed(JoypadButton::A, true));
        assert!(!joypad.set_button_pressed(JoypadButton::A, true));
        assert!(!joypad.set_button_pressed(JoypadButton::A, false));
    }

    #[test]
    fn selecting_row_with_pressed_button_returns_interrupt_request() {
        let mut joypad = Joypad::default();
        joypad.set_button_pressed(JoypadButton::A, true);
        joypad.write_p1(0x30);

        assert!(joypad.write_p1(0x10));
    }
}
