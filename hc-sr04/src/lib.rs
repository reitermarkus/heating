use std::time::{Instant, Duration};
use std::thread;

use ordered_float::NotNan;
use rppal::gpio::{self, OutputPin, InputPin, Level::*, Trigger};

const TEMPERATURE: f64 = 15.5; // °C
const SPEED_OF_SOUND: f64 = 331.5 + 0.6 * TEMPERATURE; // m/s

#[derive(Debug)]
pub enum Error {
  NoRisingEdgeDetected,
  NoFallingEdgeDetected,
  Gpio(gpio::Error),
}

#[derive(Debug)]
pub struct HcSr04 {
  trigger: OutputPin,
  echo: InputPin,
}

impl HcSr04 {
  pub fn new(mut trigger: OutputPin, mut echo: InputPin) -> Result<HcSr04, Error> {
    trigger.set_low();
    echo.set_interrupt(Trigger::Both).map_err(Error::Gpio)?;

    Ok(HcSr04 { trigger, echo })
  }

  /// Triggers an ultrasonic measurement and returns the distance in meters.
  pub fn measure(&mut self) -> Result<NotNan<f64>, Error> {
    self.trigger.set_high();
    thread::sleep(Duration::from_micros(10));
    self.trigger.set_low();

    let mut start = None;
    let mut stop = None;

    loop {
      match self.echo.poll_interrupt(false, Some(Duration::from_millis(100))).map_err(Error::Gpio)? {
        Some(High) => {
          if start.is_none() {
            start = Some(Instant::now());
          }
        },
        Some(Low) => {
          if start.is_some() && stop.is_none() {
            stop = Some(Instant::now())
          }
        },
        None => break,
      }
    }

    let start = start.ok_or(Error::NoRisingEdgeDetected)?;
    let stop = stop.ok_or(Error::NoFallingEdgeDetected)?;

    let duration = stop - start;

    let echo_length = duration.as_secs() as f64 + f64::from(duration.subsec_nanos()) * 1e-9;
    let distance = echo_length / 2.0 * SPEED_OF_SOUND;

    Ok(NotNan::from(distance))
  }
}
