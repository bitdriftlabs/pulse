// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use protobuf_codegen::Customize;
use std::path::Path;

const GENERATED_HEADER: &str = r"// proto - bitdrift's client/server API definitions
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code and APIs are governed by a source available license that can be found in
// the LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt
";

fn generate_directory(root_path: &Path, partial_path: &Path) {
  for file in std::fs::read_dir(root_path.join(partial_path)).unwrap() {
    let file = file.unwrap();
    if file.file_type().unwrap().is_dir() {
      generate_directory(root_path, &partial_path.join(file.file_name()));
    } else if file.file_type().unwrap().is_file() {
      std::fs::create_dir_all(Path::new("src/protos").join(partial_path)).unwrap();
      protobuf_codegen::Codegen::new()
        .protoc()
        .customize(
          Customize::default()
            .gen_mod_rs(false)
            .tokio_bytes(true)
            .tokio_bytes_for_string(true)
            .oneofs_non_exhaustive(false)
            .file_header(GENERATED_HEADER.to_string()),
        )
        .includes(["proto/", "../api/thirdparty/", "../api/src/"])
        .inputs([file.path()])
        .out_dir(Path::new("src/protos").join(partial_path))
        .capture_stderr()
        .run_from_script();
    }
  }
}

fn main() {
  if std::env::var("SKIP_PROTO_GEN").is_ok() {
    return;
  }

  println!("cargo:rerun-if-changed=proto/");

  generate_directory(Path::new("proto"), Path::new(""));
}
