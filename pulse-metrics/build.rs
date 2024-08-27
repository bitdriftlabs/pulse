// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

fn main() {
  println!("cargo:rerun-if-changed=src/pipeline/processor/aggregation/cm_quantile_test.c");

  cc::Build::new()
    .file("src/pipeline/processor/aggregation/cm_quantile_test.c")
    .compile("cm_quantile_test_c");
}
