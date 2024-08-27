// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use protobuf::MessageFull;
use pulse_protobuf::protos::pulse::config::common::v1::common::env_or_inline::Data_type;
use pulse_protobuf::protos::pulse::config::common::v1::common::{
  env_inline_or_file,
  EnvInlineOrFile,
  EnvOrInline,
};
use time::Duration;

pub const CONTENT_TYPE_PROTOBUF: &str = "application/x-protobuf";

// Convert an EnvOrInline proto to an optional string.
pub fn env_or_inline_to_string(env_or_inline: &EnvOrInline) -> Option<String> {
  match env_or_inline.data_type.as_ref().expect("pgv") {
    Data_type::EnvVar(env_var) => std::env::var(env_var).ok(),
    Data_type::Inline(inline) => Some(inline.to_string()),
  }
}

// Convert an EnvInlineOrFile to an optional string.
pub fn env_inline_or_file_to_string(env_inline_or_file: &EnvInlineOrFile) -> Option<String> {
  match env_inline_or_file.data_type.as_ref().expect("pgv") {
    env_inline_or_file::Data_type::EnvOrInline(env_or_inline) => {
      env_or_inline_to_string(env_or_inline)
    },
    env_inline_or_file::Data_type::FilePath(file_path) => std::fs::read_to_string(file_path).ok(),
  }
}

// Convert a YAML value to a proto by round tripping through JSON, and then run PGV on it.
pub fn yaml_value_to_proto<T: MessageFull>(value: &serde_yaml::Value) -> anyhow::Result<T> {
  let json = serde_json::to_string_pretty(&value).unwrap();
  let mut message = T::new();
  protobuf_json_mapping::merge_from_str(&mut message, &json)?;

  // Run PGV
  bd_pgv::proto_validate::validate(&message)?;

  Ok(message)
}

// Convert a YAML string to a proto by round tripping through JSON, and then run PGV on it.
pub fn yaml_to_proto<T: MessageFull>(yaml: &str) -> anyhow::Result<T> {
  // Go from YAML -> JSON -> Rust Protobuf.
  let yaml: serde_yaml::Value = serde_yaml::from_str(yaml)?;
  yaml_value_to_proto(&yaml)
}

//
// ProtoDurationTimeDuration
//

// Helper to take a google.protobuf.Duration and unwrap it with a default.
pub trait ProtoDurationToStdDuration {
  fn unwrap_duration_or(&self, default: Duration) -> Duration;
}

impl ProtoDurationToStdDuration
  for protobuf::MessageField<protobuf::well_known_types::duration::Duration>
{
  fn unwrap_duration_or(&self, default: Duration) -> Duration {
    self
      .as_ref()
      .map_or(default, bd_time::ProtoDurationExt::to_time_duration)
  }
}
