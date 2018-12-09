use measurements::Volume;

#[derive(Debug)]
pub struct Level {
  pub(crate) volume: Volume,
  pub(crate) percentage: f64,
}

impl Level {
  pub fn volume(&self) -> Volume {
    self.volume
  }

  pub fn percentage(&self) -> f64 {
    self.percentage
  }
}

impl From<Level> for f64 {
  fn from(level: Level) -> Self {
    level.percentage
  }
}

impl From<Level> for Volume {
  fn from(level: Level) -> Self {
    level.volume
  }
}
