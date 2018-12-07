use std::time::{Instant, Duration};
use std::sync::mpsc::channel;
use std::thread;

extern crate medianheap;
extern crate ordered_float;

use medianheap::MedianHeap;
use ordered_float::NotNan;

mod tank;
use tank::Tank;

mod cuboid_tank;
use cuboid_tank::CuboidTank;

extern crate sysfs_gpio;
use sysfs_gpio::{Direction, Pin, Edge::*};

const TEMPERATURE: f64 = 15.5; // °C
const SPEED_OF_SOUND: f64 = 331.5 + 0.6 * TEMPERATURE; // m/s

fn measure() -> Option<f64> {
  let trigger = Pin::new(17);
  let echo = Pin::new(18);

  let (sender, receiver) = channel();

  trigger.with_exported(|| {
    echo.with_exported(|| {
      trigger.set_direction(Direction::Out)?;
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

        let duration = stop - start;
        let echo_length = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
        let distance = echo_length / 2.0 * SPEED_OF_SOUND; // m
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

fn main() {
  let tank = CuboidTank::new(298.0, 148.0, 150.0);

  println!("Volume: {}", tank.volume());
  println!("Level: {}", tank.level());

  let mut heap = MedianHeap::new();

  for _ in 0..100 {
    let measurement = measure();

    println!("Length: {:?}", measurement);

    if let Some(measurement) = measurement {
      heap.push(NotNan::from((measurement * 1000.0).round() / 1000.0))
    }
  }

  println!("Median: {:?}", heap.median());
}
