// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pulse_metrics::protos::carbon;
use std::hash::{Hash, Hasher};

fn criterion_benchmark(c: &mut Criterion) {
  let line: bytes::Bytes = r#"new-york.power.usage 42422 123456 source=local-_-host.edu data-center_.="dc1 and dc2!@#$/%^&*()""#.into();
  let parsed = carbon::parse(&line).unwrap();

  c.bench_function("carbon parsing", |b| {
    b.iter(|| carbon::parse(black_box(&line)));
  });

  c.bench_function("carbon hash", |b| {
    b.iter(|| {
      let mut hasher = ahash::AHasher::default();
      #[allow(clippy::unit_arg)]
      black_box(parsed.get_id().hash(&mut hasher));
      hasher.finish()
    });
  });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
