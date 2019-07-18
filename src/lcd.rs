use fc113::Fc113;

#[repr(usize)]
pub enum Symbol {
  Liter,
  PercentLeft,
  PercentRight,
  Droplet,
  OeLowercase,
}

impl From<Symbol> for [u8; 8] {
  fn from(s: Symbol) -> [u8; 8] {
    match s {
      Symbol::Liter => [0x02, 0x05, 0x05, 0x06, 0x0c, 0x04, 0x03, 0x00],
      Symbol::Droplet => [
        0b00100,
        0b00100,
        0b01110,
        0b01110,
        0b11111,
        0b11111,
        0b11111,
        0b01110
      ],
      Symbol::OeLowercase => [
        0b10001,
        0b00000,
        0b01110,
        0b10001,
        0b10001,
        0b10001,
        0b01110,
        0b00000,
      ],
      Symbol::PercentLeft => [0x0c, 0x12, 0x12, 0x0c, 0x01, 0x02, 0x04, 0x00],
      Symbol::PercentRight => [0x04, 0x08, 0x10, 0x06, 0x09, 0x09, 0x06, 0x00],
    }
  }
}

pub fn update_lcd(lcd: &mut Fc113, liters: f64, percent: f64) -> Result<usize, rppal::i2c::Error> {
  use self::Symbol::*;

  lcd.set_cursor(0, 0)?;
  lcd.write(&[
    &[
      b'H',
      b'e',
      b'i',
      b'z',
      OeLowercase as u8,
      b'l',
    ],
    format!("{:>7.1} ", (liters * 10.0).round() / 10.0).replace('.', ",").as_bytes(),
    &[Liter as u8, b' '],
  ].concat())?;

  lcd.set_cursor(0, 1)?;
  lcd.write(&[
    &[Droplet as u8],
    format!("{:>12.1} ", (percent * 10.0).round() / 10.0).replace('.', ",").as_bytes(),
    &[PercentLeft as u8, PercentRight as u8],
  ].concat())
}
