// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::run;

#[test]
fn basic_case() {
  let config = r"
test_cases:
- config:
    rules:
    - name: foo
      conditions:
      - metric_name:
          exact: bar
  metrics:
  - input: bar:1|c
    dropped_by: foo
  - input: baz:1|g
  ";

  run(config, None).unwrap();
}
