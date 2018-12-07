use std::error::Error;
use std::sync::mpsc::channel;
use std::time::{Instant, Duration};
use std::thread;

extern crate medianheap;
extern crate ordered_float;
extern crate rppal;
extern crate sysfs_gpio;

use medianheap::MedianHeap;
use ordered_float::NotNan;
use rppal::gpio::{Gpio, Level::*, Mode::*, Trigger};
use sysfs_gpio::{Direction, Pin, Edge::*};

mod tank;
use tank::Tank;

mod cuboid_tank;
use cuboid_tank::CuboidTank;

const TEMPERATURE: f64 = 15.5; // °C
const SPEED_OF_SOUND: f64 = 331.5 + 0.6 * TEMPERATURE; // m/s

const TRIGGER_PIN: u8 = 17;
const ECHO_PIN:    u8 = 18;

fn measure_native(gpio: &mut Gpio) -> Option<f64> {
  gpio.write(TRIGGER_PIN, High);
  thread::sleep(Duration::from_micros(10));
  gpio.write(TRIGGER_PIN, Low);

  let mut start = None;
  let mut stop = None;

  loop {
    match gpio.poll_interrupt(ECHO_PIN, false, Some(Duration::from_millis(200))) {
      Ok(Some(High)) => {
        if start.is_none() {
          start = Some(Instant::now());
        }
      },
      Ok(Some(Low)) => {
        if start.is_some() && stop.is_none() {
          stop = Some(Instant::now())
        }
      },
      Ok(None) => break,
      Err(err) => {
        eprintln!("Error: {}", err);
        return None
      },
    }
  }

  if start.is_none() {
    eprintln!("No rising edge detected.");
  }

  if stop.is_none() {
    eprintln!("No falling edge detected.");
  }

  start.and_then(|start| stop.map(|stop| echo_duration_to_m(stop - start)))
}

fn measure_sysfs() -> Option<f64> {
  let trigger = Pin::new(TRIGGER_PIN as u64);
  let echo = Pin::new(ECHO_PIN as u64);

  let (sender, receiver) = channel();

  trigger.with_exported(|| {
    echo.with_exported(|| {
      trigger.set_direction(Direction::Out)?;
      trigger.set_value(0)?;
      echo.set_direction(Direction::In)?;

      let mut poller = echo.get_poller()?;

      let t = thread::spawn(move || -> sysfs_gpio::Result<()> {
        echo.set_edge(RisingEdge)?;

        let start = match poller.poll(10_000)? {
          Some(_) => Instant::now(),
          None => {
            sender.send(None).unwrap();
            return Ok(());
          },
        };

        echo.set_edge(FallingEdge)?;

        let stop = match poller.poll(10_000)? {
          Some(_) => Instant::now(),
          None => {
            sender.send(None).unwrap();
            return Ok(());
          },
        };

        let distance = echo_duration_to_m(stop - start);
        sender.send(Some(distance)).unwrap();

        Ok(())
      });

      trigger.set_value(1)?;
      thread::sleep(Duration::new(0, 10_000)); // 10 µs
      trigger.set_value(0)?;

      t.join().unwrap()
    })
  }).unwrap();

  receiver.recv().unwrap()
}

#[inline(always)]
fn echo_duration_to_m(duration: Duration) -> f64 {
  let echo_length = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
  echo_length / 2.0 * SPEED_OF_SOUND // m
}

fn main() -> Result<(), Box<Error>> {
  let mut native_heap = MedianHeap::new();

  {
    let mut gpio = Gpio::new()?;

    gpio.set_mode(TRIGGER_PIN, Output);
    gpio.write(TRIGGER_PIN, Low);
    gpio.set_mode(ECHO_PIN, Input);

    gpio.set_interrupt(ECHO_PIN, Trigger::Both)?;

    for _ in 0..10 {
      if let Some(distance) = measure_native(&mut gpio) {
        println!("Distance: {:?}", distance);

        native_heap.push(NotNan::from((distance * 1000.0).round() / 10.0));
      }
    }
  }

  println!();

  let mut sysfs_heap = MedianHeap::new();

  for _ in 0..10 {
    if let Some(distance) = measure_sysfs() {
      println!("Distance: {:?}", distance);

      sysfs_heap.push(NotNan::from((distance * 1000.0).round() / 10.0))
    }
  }

  println!();

  println!("Native Median: {:?}", native_heap.median());
  println!("SysFS Median: {:?}", sysfs_heap.median());

  let mut tank = CuboidTank::new(298.0, 148.0, 150.0);

  println!("Volume: {}", tank.volume());

  let filled_height = tank.height - native_heap.median().unwrap().into_inner();

  tank.set_filled_height(filled_height);

  println!("Level: {}", tank.level());

  println!("Filled Volume: {}", tank.level() * tank.volume());

  Ok(())
}
