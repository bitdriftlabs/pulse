pub mod inflow;
pub mod inflow_common;
pub mod k8s_prom;
pub mod metric_generator;
pub mod otlp;
pub mod prom_remote_write;
pub mod wire;

use super::super::common::v1::common;
use bd_pgv::generated::protos::validate;
