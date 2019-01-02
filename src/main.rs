use std::sync::mpsc::channel;
use std::time::{Instant, Duration};
use std::thread;

use medianheap::MedianHeap;
use measurements::Length;
use ordered_float::NotNan;
use rppal::gpio::{self, Gpio, OutputPin, InputPin, Level::*, Trigger};
use simple_signal::{self, Signal};
use vessel::CuboidTank;
use vessel::tank::Tank;

const TEMPERATURE: f64 = 15.5; // °C
const SPEED_OF_SOUND: f64 = 331.5 + 0.6 * TEMPERATURE; // m/s

const TRIGGER_PIN: u8 = 17;
const ECHO_PIN:    u8 = 18;

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
    echo.set_interrupt(Trigger::Both).map_err(|err| Error::Gpio(err))?;

    Ok(HcSr04 { trigger, echo })
  }

  pub fn measure(&mut self) -> Result<f64, Error> {
    self.trigger.set_high();
    thread::sleep(Duration::from_micros(10));
    self.trigger.set_low();

    let mut start = None;
    let mut stop = None;

    loop {
      match self.echo.poll_interrupt(false, Some(Duration::from_millis(100))).map_err(|err| Error::Gpio(err))? {
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

    #[inline(always)]
    fn echo_duration_to_m(duration: Duration) -> f64 {
      let echo_length = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
      echo_length / 2.0 * SPEED_OF_SOUND // m
    }

    Ok(echo_duration_to_m(stop - start))
  }
}

fn main() {
  let gpio = Gpio::new().expect("failed to access GPIO");

  let (tx, rx) = channel();

  let (sig_tx, sig_rx) = channel();

  simple_signal::set_handler(&[Signal::Int], move |signals| {
    println!("Caught: {:?}", signals);
    sig_tx.send(true).unwrap();
    sig_tx.send(true).unwrap();
  });

  let trigger = gpio.get(TRIGGER_PIN).unwrap().into_output();
  let echo = gpio.get(ECHO_PIN).unwrap().into_input();

  let mut sensor = HcSr04::new(trigger, echo).expect("failed to set up sensor");

  thread::spawn(move || {
    loop {
      if sig_rx.try_recv().unwrap_or(false) {
        break
      }

      if let Ok(distance) = sensor.measure() {
        tx.send(distance).expect("failed to send distance to main thread")
      }
    }
  });

  let tank = CuboidTank::new(Length::from_centimeters(298.0), Length::from_centimeters(148.0), Length::from_centimeters(150.0));
  let mut heap = MedianHeap::with_max_size(10000);
  let sensor_offset = Length::from_centimeters(4.0);

  loop {
    let distance = match rx.recv() {
      Ok(distance) => distance,
      _ => break
    };

    heap.push(NotNan::from(distance));

    let buffer = 100;

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
