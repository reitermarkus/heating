use tank::Tank;

#[derive(Debug)]
pub struct CuboidTank {
  pub length: f64,
  pub width: f64,
  pub height: f64,
  pub filled_height: f64,
}

impl CuboidTank {
  pub fn new<T: Into<f64>>(length: T, width: T, height: T) -> Self {
    Self { length: length.into(), width: width.into(), height: height.into(), filled_height: 0.0 }
  }

  pub fn set_filled_height<T: Into<f64>>(&mut self, filled_height: T) {
    self.filled_height = filled_height.into();
  }
}

impl Tank for CuboidTank {
  fn volume(&self) -> f64 {
    self.length * self.width * self.height / 1000.0
  }

  fn level(&self) -> f64 {
    self.filled_height / self.height
  }
}
