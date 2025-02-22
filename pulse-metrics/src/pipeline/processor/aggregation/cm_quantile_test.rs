// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;

#[test]
fn add() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  cm.add_sample(100.0);
}

#[test]
fn cm_add_loop() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  for i in 0 .. 1000u64 {
    cm.add_sample(i as f64);
  }
}

#[test]
fn query() {
  let cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  assert_eq!(0.0, cm.query(0.5));
}

#[test]
fn add_query() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  cm.add_sample(100.0);
  assert_eq!(100.0, cm.query(0.5));
}

#[test]
fn add_negative_query() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  cm.add_sample(-100.0);
  assert_eq!(-100.0, cm.query(0.5));
}

#[test]
fn add_loop_query() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  for i in 0 .. 100_000u64 {
    cm.add_sample(i as f64);
  }
  cm.flush();

  let val = cm.query(0.5);
  assert!((50000.0 - 1000.0 ..= 50000.0 + 1000.0).contains(&val));

  let val = cm.query(0.9);
  assert!((90000.0 - 1000.0 ..= 90000.0 + 1000.0).contains(&val));

  let val = cm.query(0.99);
  assert!((99000.0 - 1000.0 ..= 99000.0 + 1000.0).contains(&val));
}

#[test]
fn add_loop_rev_query() {
  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  for i in (0 .. 100_000u64).rev() {
    cm.add_sample(i as f64);
  }
  cm.flush();

  let val = cm.query(0.5);
  assert!((50000.0 - 1000.0 ..= 50000.0 + 1000.0).contains(&val));

  let val = cm.query(0.9);
  assert!((90000.0 - 1000.0 ..= 90000.0 + 1000.0).contains(&val));

  let val = cm.query(0.99);
  assert!((99000.0 - 1000.0 ..= 99000.0 + 1000.0).contains(&val));
}

// See the C file for why this exists.
unsafe extern "C" {
  fn cm_quantile_test_initialize_c();
  fn cm_quantile_test_random_c() -> u64;
}

#[test]
fn add_loop_random_query() {
  unsafe { cm_quantile_test_initialize_c() };

  let mut cm = Quantile::new(0.01, vec![0.5, 0.9, 0.99]);
  for _ in 0 .. 100_000 {
    cm.add_sample(unsafe { cm_quantile_test_random_c() } as f64);
  }
  cm.flush();

  let val = cm.query(0.5);
  assert!((1_073_741_823.0 - 21_474_836.0 ..= 1_073_741_823.0 + 21_474_836.0).contains(&val));

  let val = cm.query(0.9);
  assert!((1_932_735_282.0 - 21_474_836.0 ..= 1_932_735_282.0 + 21_474_836.0).contains(&val));

  let val = cm.query(0.99);
  assert!((2_126_008_810.0 - 21_474_836.0 ..= 2_126_008_810.0 + 21_474_836.0).contains(&val));
}
