use measurements::{Length, Volume};

use crate::level::Level;

pub trait Tank {
  fn volume(&self) -> Volume;
  fn level(&self, filling_height: Length) -> Level;
}
