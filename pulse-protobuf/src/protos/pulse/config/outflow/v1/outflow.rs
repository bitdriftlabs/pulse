// proto - bitdrift's client/server API definitions
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code and APIs are governed by a source available license that can be found in
// the LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

// This file is generated by rust-protobuf 4.0.0-alpha.0. Do not edit
// .proto file is parsed by protoc 29.0
// @generated

// https://github.com/rust-lang/rust-clippy/issues/702
#![allow(unknown_lints)]
#![allow(clippy::all)]

#![allow(unused_attributes)]
#![cfg_attr(rustfmt, rustfmt::skip)]

#![allow(dead_code)]
#![allow(missing_docs)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(trivial_casts)]
#![allow(unused_results)]
#![allow(unused_mut)]

//! Generated file from `pulse/config/outflow/v1/outflow.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.outflow.v1.OutflowConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct OutflowConfig {
    // message oneof groups
    pub config_type: ::std::option::Option<outflow_config::Config_type>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.OutflowConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a OutflowConfig {
    fn default() -> &'a OutflowConfig {
        <OutflowConfig as ::protobuf::Message>::default_instance()
    }
}

impl OutflowConfig {
    pub fn new() -> OutflowConfig {
        ::std::default::Default::default()
    }

    // .pulse.config.outflow.v1.NullClientConfig null_outflow = 1;

    pub fn null_outflow(&self) -> &super::wire::NullClientConfig {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(ref v)) => v,
            _ => <super::wire::NullClientConfig as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_null_outflow(&mut self) {
        self.config_type = ::std::option::Option::None;
    }

    pub fn has_null_outflow(&self) -> bool {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_null_outflow(&mut self, v: super::wire::NullClientConfig) {
        self.config_type = ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(v))
    }

    // Mutable pointer to the field.
    pub fn mut_null_outflow(&mut self) -> &mut super::wire::NullClientConfig {
        if let ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(_)) = self.config_type {
        } else {
            self.config_type = ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(super::wire::NullClientConfig::new()));
        }
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_null_outflow(&mut self) -> super::wire::NullClientConfig {
        if self.has_null_outflow() {
            match self.config_type.take() {
                ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(v)) => v,
                _ => panic!(),
            }
        } else {
            super::wire::NullClientConfig::new()
        }
    }

    // .pulse.config.outflow.v1.UnixClientConfig unix = 2;

    pub fn unix(&self) -> &super::wire::UnixClientConfig {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Unix(ref v)) => v,
            _ => <super::wire::UnixClientConfig as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_unix(&mut self) {
        self.config_type = ::std::option::Option::None;
    }

    pub fn has_unix(&self) -> bool {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Unix(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_unix(&mut self, v: super::wire::UnixClientConfig) {
        self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Unix(v))
    }

    // Mutable pointer to the field.
    pub fn mut_unix(&mut self) -> &mut super::wire::UnixClientConfig {
        if let ::std::option::Option::Some(outflow_config::Config_type::Unix(_)) = self.config_type {
        } else {
            self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Unix(super::wire::UnixClientConfig::new()));
        }
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Unix(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_unix(&mut self) -> super::wire::UnixClientConfig {
        if self.has_unix() {
            match self.config_type.take() {
                ::std::option::Option::Some(outflow_config::Config_type::Unix(v)) => v,
                _ => panic!(),
            }
        } else {
            super::wire::UnixClientConfig::new()
        }
    }

    // .pulse.config.outflow.v1.TcpClientConfig tcp = 3;

    pub fn tcp(&self) -> &super::wire::TcpClientConfig {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Tcp(ref v)) => v,
            _ => <super::wire::TcpClientConfig as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_tcp(&mut self) {
        self.config_type = ::std::option::Option::None;
    }

    pub fn has_tcp(&self) -> bool {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Tcp(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_tcp(&mut self, v: super::wire::TcpClientConfig) {
        self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Tcp(v))
    }

    // Mutable pointer to the field.
    pub fn mut_tcp(&mut self) -> &mut super::wire::TcpClientConfig {
        if let ::std::option::Option::Some(outflow_config::Config_type::Tcp(_)) = self.config_type {
        } else {
            self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Tcp(super::wire::TcpClientConfig::new()));
        }
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Tcp(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_tcp(&mut self) -> super::wire::TcpClientConfig {
        if self.has_tcp() {
            match self.config_type.take() {
                ::std::option::Option::Some(outflow_config::Config_type::Tcp(v)) => v,
                _ => panic!(),
            }
        } else {
            super::wire::TcpClientConfig::new()
        }
    }

    // .pulse.config.outflow.v1.UdpClientConfig udp = 4;

    pub fn udp(&self) -> &super::wire::UdpClientConfig {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Udp(ref v)) => v,
            _ => <super::wire::UdpClientConfig as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_udp(&mut self) {
        self.config_type = ::std::option::Option::None;
    }

    pub fn has_udp(&self) -> bool {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Udp(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_udp(&mut self, v: super::wire::UdpClientConfig) {
        self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Udp(v))
    }

    // Mutable pointer to the field.
    pub fn mut_udp(&mut self) -> &mut super::wire::UdpClientConfig {
        if let ::std::option::Option::Some(outflow_config::Config_type::Udp(_)) = self.config_type {
        } else {
            self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Udp(super::wire::UdpClientConfig::new()));
        }
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::Udp(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_udp(&mut self) -> super::wire::UdpClientConfig {
        if self.has_udp() {
            match self.config_type.take() {
                ::std::option::Option::Some(outflow_config::Config_type::Udp(v)) => v,
                _ => panic!(),
            }
        } else {
            super::wire::UdpClientConfig::new()
        }
    }

    // .pulse.config.outflow.v1.PromRemoteWriteClientConfig prom_remote_write = 5;

    pub fn prom_remote_write(&self) -> &super::prom_remote_write::PromRemoteWriteClientConfig {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(ref v)) => v,
            _ => <super::prom_remote_write::PromRemoteWriteClientConfig as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_prom_remote_write(&mut self) {
        self.config_type = ::std::option::Option::None;
    }

    pub fn has_prom_remote_write(&self) -> bool {
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_prom_remote_write(&mut self, v: super::prom_remote_write::PromRemoteWriteClientConfig) {
        self.config_type = ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(v))
    }

    // Mutable pointer to the field.
    pub fn mut_prom_remote_write(&mut self) -> &mut super::prom_remote_write::PromRemoteWriteClientConfig {
        if let ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(_)) = self.config_type {
        } else {
            self.config_type = ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(super::prom_remote_write::PromRemoteWriteClientConfig::new()));
        }
        match self.config_type {
            ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_prom_remote_write(&mut self) -> super::prom_remote_write::PromRemoteWriteClientConfig {
        if self.has_prom_remote_write() {
            match self.config_type.take() {
                ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(v)) => v,
                _ => panic!(),
            }
        } else {
            super::prom_remote_write::PromRemoteWriteClientConfig::new()
        }
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(5);
        let mut oneofs = ::std::vec::Vec::with_capacity(1);
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, super::wire::NullClientConfig>(
            "null_outflow",
            OutflowConfig::has_null_outflow,
            OutflowConfig::null_outflow,
            OutflowConfig::mut_null_outflow,
            OutflowConfig::set_null_outflow,
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, super::wire::UnixClientConfig>(
            "unix",
            OutflowConfig::has_unix,
            OutflowConfig::unix,
            OutflowConfig::mut_unix,
            OutflowConfig::set_unix,
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, super::wire::TcpClientConfig>(
            "tcp",
            OutflowConfig::has_tcp,
            OutflowConfig::tcp,
            OutflowConfig::mut_tcp,
            OutflowConfig::set_tcp,
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, super::wire::UdpClientConfig>(
            "udp",
            OutflowConfig::has_udp,
            OutflowConfig::udp,
            OutflowConfig::mut_udp,
            OutflowConfig::set_udp,
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, super::prom_remote_write::PromRemoteWriteClientConfig>(
            "prom_remote_write",
            OutflowConfig::has_prom_remote_write,
            OutflowConfig::prom_remote_write,
            OutflowConfig::mut_prom_remote_write,
            OutflowConfig::set_prom_remote_write,
        ));
        oneofs.push(outflow_config::Config_type::generated_oneof_descriptor_data());
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<OutflowConfig>(
            "OutflowConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for OutflowConfig {
    const NAME: &'static str = "OutflowConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.config_type = ::std::option::Option::Some(outflow_config::Config_type::NullOutflow(is.read_message()?));
                },
                18 => {
                    self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Unix(is.read_message()?));
                },
                26 => {
                    self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Tcp(is.read_message()?));
                },
                34 => {
                    self.config_type = ::std::option::Option::Some(outflow_config::Config_type::Udp(is.read_message()?));
                },
                42 => {
                    self.config_type = ::std::option::Option::Some(outflow_config::Config_type::PromRemoteWrite(is.read_message()?));
                },
                tag => {
                    ::protobuf::rt::read_unknown_or_skip_group(tag, is, self.special_fields.mut_unknown_fields())?;
                },
            };
        }
        ::std::result::Result::Ok(())
    }

    // Compute sizes of nested messages
    #[allow(unused_variables)]
    fn compute_size(&self) -> u64 {
        let mut my_size = 0;
        if let ::std::option::Option::Some(ref v) = self.config_type {
            match v {
                &outflow_config::Config_type::NullOutflow(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
                &outflow_config::Config_type::Unix(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
                &outflow_config::Config_type::Tcp(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
                &outflow_config::Config_type::Udp(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
                &outflow_config::Config_type::PromRemoteWrite(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
            };
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let ::std::option::Option::Some(ref v) = self.config_type {
            match v {
                &outflow_config::Config_type::NullOutflow(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
                },
                &outflow_config::Config_type::Unix(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
                },
                &outflow_config::Config_type::Tcp(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(3, v, os)?;
                },
                &outflow_config::Config_type::Udp(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(4, v, os)?;
                },
                &outflow_config::Config_type::PromRemoteWrite(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(5, v, os)?;
                },
            };
        }
        os.write_unknown_fields(self.special_fields.unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn special_fields(&self) -> &::protobuf::SpecialFields {
        &self.special_fields
    }

    fn mut_special_fields(&mut self) -> &mut ::protobuf::SpecialFields {
        &mut self.special_fields
    }

    fn new() -> OutflowConfig {
        OutflowConfig::new()
    }

    fn clear(&mut self) {
        self.config_type = ::std::option::Option::None;
        self.config_type = ::std::option::Option::None;
        self.config_type = ::std::option::Option::None;
        self.config_type = ::std::option::Option::None;
        self.config_type = ::std::option::Option::None;
        self.special_fields.clear();
    }

    fn default_instance() -> &'static OutflowConfig {
        static instance: OutflowConfig = OutflowConfig {
            config_type: ::std::option::Option::None,
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for OutflowConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("OutflowConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for OutflowConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for OutflowConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

/// Nested message and enums of message `OutflowConfig`
pub mod outflow_config {

    #[derive(Clone,PartialEq,Debug)]
    // @@protoc_insertion_point(oneof:pulse.config.outflow.v1.OutflowConfig.config_type)
    pub enum Config_type {
        // @@protoc_insertion_point(oneof_field:pulse.config.outflow.v1.OutflowConfig.null_outflow)
        NullOutflow(super::super::wire::NullClientConfig),
        // @@protoc_insertion_point(oneof_field:pulse.config.outflow.v1.OutflowConfig.unix)
        Unix(super::super::wire::UnixClientConfig),
        // @@protoc_insertion_point(oneof_field:pulse.config.outflow.v1.OutflowConfig.tcp)
        Tcp(super::super::wire::TcpClientConfig),
        // @@protoc_insertion_point(oneof_field:pulse.config.outflow.v1.OutflowConfig.udp)
        Udp(super::super::wire::UdpClientConfig),
        // @@protoc_insertion_point(oneof_field:pulse.config.outflow.v1.OutflowConfig.prom_remote_write)
        PromRemoteWrite(super::super::prom_remote_write::PromRemoteWriteClientConfig),
    }

    impl ::protobuf::Oneof for Config_type {
    }

    impl ::protobuf::OneofFull for Config_type {
        fn descriptor() -> ::protobuf::reflect::OneofDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::OneofDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| <super::OutflowConfig as ::protobuf::MessageFull>::descriptor().oneof_by_name("config_type").unwrap()).clone()
        }
    }

    impl Config_type {
        pub(in super) fn generated_oneof_descriptor_data() -> ::protobuf::reflect::GeneratedOneofDescriptorData {
            ::protobuf::reflect::GeneratedOneofDescriptorData::new::<Config_type>("config_type")
        }
    }
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n%pulse/config/outflow/v1/outflow.proto\x12\x17pulse.config.outflow.v1\
    \x1a/pulse/config/outflow/v1/prom_remote_write.proto\x1a\"pulse/config/o\
    utflow/v1/wire.proto\x1a\x17validate/validate.proto\"\x94\x03\n\rOutflow\
    Config\x12N\n\x0cnull_outflow\x18\x01\x20\x01(\x0b2).pulse.config.outflo\
    w.v1.NullClientConfigH\0R\x0bnullOutflow\x12?\n\x04unix\x18\x02\x20\x01(\
    \x0b2).pulse.config.outflow.v1.UnixClientConfigH\0R\x04unix\x12<\n\x03tc\
    p\x18\x03\x20\x01(\x0b2(.pulse.config.outflow.v1.TcpClientConfigH\0R\x03\
    tcp\x12<\n\x03udp\x18\x04\x20\x01(\x0b2(.pulse.config.outflow.v1.UdpClie\
    ntConfigH\0R\x03udp\x12b\n\x11prom_remote_write\x18\x05\x20\x01(\x0b24.p\
    ulse.config.outflow.v1.PromRemoteWriteClientConfigH\0R\x0fpromRemoteWrit\
    eB\x12\n\x0bconfig_type\x12\x03\xf8B\x01b\x06proto3\
";

/// `FileDescriptorProto` object which was a source for this generated file
fn file_descriptor_proto() -> &'static ::protobuf::descriptor::FileDescriptorProto {
    static file_descriptor_proto_lazy: ::protobuf::rt::Lazy<::protobuf::descriptor::FileDescriptorProto> = ::protobuf::rt::Lazy::new();
    file_descriptor_proto_lazy.get(|| {
        ::protobuf::Message::parse_from_bytes(file_descriptor_proto_data).unwrap()
    })
}

/// `FileDescriptor` object which allows dynamic access to files
pub fn file_descriptor() -> &'static ::protobuf::reflect::FileDescriptor {
    static generated_file_descriptor_lazy: ::protobuf::rt::Lazy<::protobuf::reflect::GeneratedFileDescriptor> = ::protobuf::rt::Lazy::new();
    static file_descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::FileDescriptor> = ::protobuf::rt::Lazy::new();
    file_descriptor.get(|| {
        let generated_file_descriptor = generated_file_descriptor_lazy.get(|| {
            let mut deps = ::std::vec::Vec::with_capacity(3);
            deps.push(super::prom_remote_write::file_descriptor().clone());
            deps.push(super::wire::file_descriptor().clone());
            deps.push(super::validate::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(1);
            messages.push(OutflowConfig::generated_message_descriptor_data());
            let mut enums = ::std::vec::Vec::with_capacity(0);
            ::protobuf::reflect::GeneratedFileDescriptor::new_generated(
                file_descriptor_proto(),
                deps,
                messages,
                enums,
            )
        });
        ::protobuf::reflect::FileDescriptor::new_generated_2(generated_file_descriptor)
    })
}
