use std::sync::mpsc::channel;
use std::time::{Instant, Duration};
use std::thread;

use medianheap::MedianHeap;
use measurements::Length;
use ordered_float::NotNan;
use rppal::gpio::{self, Gpio, Level::*, Mode::*, Trigger};
use vessel::CuboidTank;
use vessel::tank::Tank;

const TEMPERATURE: f64 = 15.5; // °C
const SPEED_OF_SOUND: f64 = 331.5 + 0.6 * TEMPERATURE; // m/s

const TRIGGER_PIN: u8 = 17;
const ECHO_PIN:    u8 = 18;

#[derive(Debug)]
enum Error {
  NoRisingEdgeDetected,
  NoFallingEdgeDetected,
  GpioError(gpio::Error),
}

fn measure_native(gpio: &mut Gpio) -> Result<f64, Error> {
  gpio.write(TRIGGER_PIN, High);
  thread::sleep(Duration::from_micros(10));
  gpio.write(TRIGGER_PIN, Low);

  let mut start = None;
  let mut stop = None;

  loop {
    match gpio.poll_interrupt(ECHO_PIN, false, Some(Duration::from_millis(100))).map_err(|err| Error::GpioError(err))? {
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

  Ok(echo_duration_to_m(stop - start))
}

#[inline(always)]
fn echo_duration_to_m(duration: Duration) -> f64 {
  let echo_length = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
  echo_length / 2.0 * SPEED_OF_SOUND // m
}

fn main() {
  let mut gpio = Gpio::new().expect("failed to access GPIO");

  gpio.set_mode(TRIGGER_PIN, Output);
  gpio.write(TRIGGER_PIN, Low);
  gpio.set_mode(ECHO_PIN, Input);

  gpio.set_interrupt(ECHO_PIN, Trigger::Both).expect("failed to set interrupt on echo pin");

  let (tx, rx) = channel();

  thread::spawn(move || {
    loop {
      if let Ok(distance) = measure_native(&mut gpio) {
        tx.send(distance).expect("failed to send distance to main thread")
      }
    }
  });

  let tank = CuboidTank::new(Length::from_centimeters(298.0), Length::from_centimeters(148.0), Length::from_centimeters(150.0));
  let mut heap = MedianHeap::with_max_size(10000);
  let sensor_offset = Length::from_centimeters(4.0);

  loop {
    let distance = rx.recv().expect("failed to received distance to measurement thread");

    heap.push(NotNan::from(distance));

    let buffer = 2000;

    if heap.len() < buffer {
      println!("Waiting for first 1000 measurements: {:>#4}/{}", heap.len(), buffer);
    } else {
      let median_distance = Length::from_millimeters((heap.median().unwrap() * 1000.0).round()) - sensor_offset;

      let filled_height = tank.height() - median_distance;

      let level = tank.level(filled_height);

      println!("Median: {}", median_distance);
      println!("Tank: {} l ({} %)", level.volume().as_liters(), level.percentage() * 100.0)
    }
  }
}
