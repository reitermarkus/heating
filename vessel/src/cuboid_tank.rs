use measurements::Length;
use measurements::Volume;

use crate::level::Level;
use crate::tank::Tank;

#[derive(Debug)]
pub struct CuboidTank {
  length: Length,
  width: Length,
  height: Length,
}

impl CuboidTank {
  pub fn new(length: Length, width: Length, height: Length) -> Self {
    Self { length, width, height }
  }

  pub fn length(&self) -> Length {
    self.length
  }

  pub fn width(&self) -> Length {
    self.width
  }

  pub fn height(&self) -> Length {
    self.height
  }
}

impl Tank for CuboidTank {
  fn volume(&self) -> Volume {
    Volume::from_liters(self.length.as_decimeters() * self.width.as_decimeters() * self.height.as_decimeters())
  }

  fn level(&self, filling_height: Length) -> Level {
    Level {
      volume: Volume::from_liters(self.length.as_decimeters() * self.width.as_decimeters() * filling_height.as_decimeters()),
      percentage: filling_height / self.height,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn volume() {
    let tank = CuboidTank::new(Length::from_meters(1.0), Length::from_meters(2.0), Length::from_meters(3.0));
    assert_eq!(tank.volume(), Volume::from_liters(6000.0));
  }
}
