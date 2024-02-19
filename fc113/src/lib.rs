use std::{thread::sleep, time::Duration};

use rppal::i2c::I2c;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Command {
  ClearDisplay   = 0b0000_0001,
  ReturnHome     = 0b0000_0010,
  EntryModeSet   = 0b0000_0100,
  DisplayControl = 0b0000_1000,
  CursorShift    = 0b0001_0000,
  FunctionSet    = 0b0010_0000,
  SetCgramAddr   = 0b0100_0000,
  SetDgramAddr   = 0b1000_0000,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DisplayCursorShift {
  DisplayMove = 0x08,
  CursorMove  = 0x00,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DisplayMove {
  Right = 0x04,
  Left  = 0x00,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Mode {
  Command = 0x00,
  Data    = 0x01,
}

#[derive(Debug)]
pub struct Fc113 {
  pub bus: I2c,
  pub rows: usize,
  pub backlight: bool,
  pub big_font: bool,
}

impl Fc113 {
  pub const ADDR: u16 = 0x27;
  pub const WIDTH: usize = 16;

  pub const ENABLE: u8 = 0b0000_0100;

  pub const DATA_LENGTH_8_BITS: u8 = 0x10;
  pub const ROWS_2: u8 = 0x08;

  pub const FONT_SIZE_5X10: u8 = 0x04;

  pub const DISPLAY_ON: u8 = 0x04;
  pub const CURSOR_ON: u8 = 0x02;
  pub const BLINK_ON: u8 = 0x01;

  pub const BACKLIGHT_ON: u8 = 0x08;

  pub const ENTRY_LEFT: u8 = 0x02;
  pub const ENTRY_SHIFT_INCREMENT: u8 = 0x01;

  pub fn new(bus: I2c, rows: usize) -> Result<Fc113, rppal::i2c::Error> {
    assert!(rows <= 2);

    let mut lcd = Fc113 { bus, rows, backlight: true, big_font: false };

    lcd.init()?;

    Ok(lcd)
  }

  fn init(&mut self) -> Result<usize, rppal::i2c::Error> {
    self.bus.set_slave_address(Self::ADDR)?;

    sleep(Duration::from_millis(50));

    self.expander_write(0)?;
    sleep(Duration::from_millis(1000));

    self.write_4_bits(0x03 << 4)?;
    sleep(Duration::from_micros(4500));

    self.write_4_bits(0x03 << 4)?;
    sleep(Duration::from_micros(4500));

    self.write_4_bits(0x03 << 4)?;
    sleep(Duration::from_micros(150));

    self.write_4_bits(0x02 << 4)?;

    self.command(
      Command::FunctionSet as u8
        | if self.rows == 2 {
          Self::ROWS_2 as u8
        } else if self.big_font {
          Self::FONT_SIZE_5X10 as u8
        } else {
          0
        },
    )?;

    self.command(Command::DisplayControl as u8 | Self::DISPLAY_ON)?;

    self.clear()?;

    self.command(Command::EntryModeSet as u8 | Self::ENTRY_LEFT)
  }

  pub fn home(&mut self) -> Result<usize, rppal::i2c::Error> {
    let res = self.command(Command::ReturnHome as u8)?;
    sleep(Duration::from_micros(2000));
    Ok(res)
  }

  pub fn clear(&mut self) -> Result<usize, rppal::i2c::Error> {
    let res = self.command(Command::ClearDisplay as u8)?;
    sleep(Duration::from_micros(2000));
    Ok(res)
  }

  fn command(&mut self, bits: u8) -> Result<usize, rppal::i2c::Error> {
    self.send(bits, Mode::Command)
  }

  pub fn write(&mut self, bytes: &[u8]) -> Result<usize, rppal::i2c::Error> {
    for byte in bytes {
      self.send(*byte, Mode::Data)?;
    }

    Ok(bytes.len())
  }

  pub fn set_cursor(&mut self, col: usize, row: usize) -> Result<usize, rppal::i2c::Error> {
    let row_offsets = [0x00, 0x40, 0x14, 0x54];
    self.command(Command::SetDgramAddr as u8 | (col + row_offsets[row]) as u8)
  }

  pub fn create_char(&mut self, location: usize, charmap: impl Into<[u8; 8]>) -> Result<usize, rppal::i2c::Error> {
    assert!(location < 8);

    let charmap = charmap.into();

    self.command(Command::SetCgramAddr as u8 | (location << 3) as u8)?;

    for part in charmap.iter() {
      self.write(&[*part])?;
    }

    Ok(8)
  }

  fn send(&mut self, bits: u8, mode: Mode) -> Result<usize, rppal::i2c::Error> {
    let bits_high = mode as u8 | (bits & 0xF0);
    let bits_low = mode as u8 | ((bits << 4) & 0xF0);

    self.write_4_bits(bits_high)?;
    self.write_4_bits(bits_low)
  }

  fn pulse_enable(&mut self, bits: u8) -> Result<usize, rppal::i2c::Error> {
    self.expander_write(bits | Self::ENABLE)?;
    sleep(Duration::from_micros(1));
    let res = self.expander_write(bits & !Self::ENABLE)?;
    sleep(Duration::from_micros(50));
    Ok(res)
  }

  fn write_4_bits(&mut self, bits: u8) -> Result<usize, rppal::i2c::Error> {
    self.expander_write(bits)?;
    self.pulse_enable(bits)
  }

  fn expander_write(&mut self, bits: u8) -> Result<usize, rppal::i2c::Error> {
    if self.backlight {
      self.bus.write(&[bits | Self::BACKLIGHT_ON])
    } else {
      self.bus.write(&[bits])
    }
  }
}
