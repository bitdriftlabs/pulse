// proto - bitdrift's client/server API definitions
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code and APIs are governed by a source available license that can be found in
// the LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

// This file is generated by rust-protobuf 4.0.0-alpha.0. Do not edit
// .proto file is parsed by protoc 27.3
// @generated

// https://github.com/rust-lang/rust-clippy/issues/702
#![allow(unknown_lints)]
#![allow(clippy::all)]

#![allow(unused_attributes)]
#![cfg_attr(rustfmt, rustfmt::skip)]

#![allow(box_pointers)]
#![allow(dead_code)]
#![allow(missing_docs)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(trivial_casts)]
#![allow(unused_results)]
#![allow(unused_mut)]

//! Generated file from `pulse/config/outflow/v1/wire.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.outflow.v1.CommonWireClientConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct CommonWireClientConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.send_to)
    pub send_to: ::protobuf::Chars,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.protocol)
    pub protocol: ::protobuf::MessageField<super::common::WireProtocol>,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.queue_policy)
    pub queue_policy: ::protobuf::MessageField<super::queue_policy::QueuePolicy>,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.batch_max_bytes)
    pub batch_max_bytes: ::std::option::Option<u64>,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.write_timeout)
    pub write_timeout: ::protobuf::MessageField<::protobuf::well_known_types::duration::Duration>,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.CommonWireClientConfig.max_in_flight)
    pub max_in_flight: ::std::option::Option<u64>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.CommonWireClientConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a CommonWireClientConfig {
    fn default() -> &'a CommonWireClientConfig {
        <CommonWireClientConfig as ::protobuf::Message>::default_instance()
    }
}

impl CommonWireClientConfig {
    pub fn new() -> CommonWireClientConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(6);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "send_to",
            |m: &CommonWireClientConfig| { &m.send_to },
            |m: &mut CommonWireClientConfig| { &mut m.send_to },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, super::common::WireProtocol>(
            "protocol",
            |m: &CommonWireClientConfig| { &m.protocol },
            |m: &mut CommonWireClientConfig| { &mut m.protocol },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, super::queue_policy::QueuePolicy>(
            "queue_policy",
            |m: &CommonWireClientConfig| { &m.queue_policy },
            |m: &mut CommonWireClientConfig| { &mut m.queue_policy },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "batch_max_bytes",
            |m: &CommonWireClientConfig| { &m.batch_max_bytes },
            |m: &mut CommonWireClientConfig| { &mut m.batch_max_bytes },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ::protobuf::well_known_types::duration::Duration>(
            "write_timeout",
            |m: &CommonWireClientConfig| { &m.write_timeout },
            |m: &mut CommonWireClientConfig| { &mut m.write_timeout },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "max_in_flight",
            |m: &CommonWireClientConfig| { &m.max_in_flight },
            |m: &mut CommonWireClientConfig| { &mut m.max_in_flight },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<CommonWireClientConfig>(
            "CommonWireClientConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for CommonWireClientConfig {
    const NAME: &'static str = "CommonWireClientConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.send_to = is.read_tokio_chars()?;
                },
                18 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.protocol)?;
                },
                26 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.queue_policy)?;
                },
                32 => {
                    self.batch_max_bytes = ::std::option::Option::Some(is.read_uint64()?);
                },
                42 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.write_timeout)?;
                },
                48 => {
                    self.max_in_flight = ::std::option::Option::Some(is.read_uint64()?);
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
        if !self.send_to.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.send_to);
        }
        if let Some(v) = self.protocol.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let Some(v) = self.queue_policy.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let Some(v) = self.batch_max_bytes {
            my_size += ::protobuf::rt::uint64_size(4, v);
        }
        if let Some(v) = self.write_timeout.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let Some(v) = self.max_in_flight {
            my_size += ::protobuf::rt::uint64_size(6, v);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if !self.send_to.is_empty() {
            os.write_string(1, &self.send_to)?;
        }
        if let Some(v) = self.protocol.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
        }
        if let Some(v) = self.queue_policy.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(3, v, os)?;
        }
        if let Some(v) = self.batch_max_bytes {
            os.write_uint64(4, v)?;
        }
        if let Some(v) = self.write_timeout.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(5, v, os)?;
        }
        if let Some(v) = self.max_in_flight {
            os.write_uint64(6, v)?;
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

    fn new() -> CommonWireClientConfig {
        CommonWireClientConfig::new()
    }

    fn clear(&mut self) {
        self.send_to.clear();
        self.protocol.clear();
        self.queue_policy.clear();
        self.batch_max_bytes = ::std::option::Option::None;
        self.write_timeout.clear();
        self.max_in_flight = ::std::option::Option::None;
        self.special_fields.clear();
    }

    fn default_instance() -> &'static CommonWireClientConfig {
        static instance: CommonWireClientConfig = CommonWireClientConfig {
            send_to: ::protobuf::Chars::new(),
            protocol: ::protobuf::MessageField::none(),
            queue_policy: ::protobuf::MessageField::none(),
            batch_max_bytes: ::std::option::Option::None,
            write_timeout: ::protobuf::MessageField::none(),
            max_in_flight: ::std::option::Option::None,
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for CommonWireClientConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("CommonWireClientConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for CommonWireClientConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for CommonWireClientConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.outflow.v1.UnixClientConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct UnixClientConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.UnixClientConfig.common)
    pub common: ::protobuf::MessageField<CommonWireClientConfig>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.UnixClientConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a UnixClientConfig {
    fn default() -> &'a UnixClientConfig {
        <UnixClientConfig as ::protobuf::Message>::default_instance()
    }
}

impl UnixClientConfig {
    pub fn new() -> UnixClientConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, CommonWireClientConfig>(
            "common",
            |m: &UnixClientConfig| { &m.common },
            |m: &mut UnixClientConfig| { &mut m.common },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<UnixClientConfig>(
            "UnixClientConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for UnixClientConfig {
    const NAME: &'static str = "UnixClientConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.common)?;
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
        if let Some(v) = self.common.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.common.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
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

    fn new() -> UnixClientConfig {
        UnixClientConfig::new()
    }

    fn clear(&mut self) {
        self.common.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static UnixClientConfig {
        static instance: UnixClientConfig = UnixClientConfig {
            common: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for UnixClientConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("UnixClientConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for UnixClientConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for UnixClientConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.outflow.v1.UdpClientConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct UdpClientConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.UdpClientConfig.common)
    pub common: ::protobuf::MessageField<CommonWireClientConfig>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.UdpClientConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a UdpClientConfig {
    fn default() -> &'a UdpClientConfig {
        <UdpClientConfig as ::protobuf::Message>::default_instance()
    }
}

impl UdpClientConfig {
    pub fn new() -> UdpClientConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, CommonWireClientConfig>(
            "common",
            |m: &UdpClientConfig| { &m.common },
            |m: &mut UdpClientConfig| { &mut m.common },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<UdpClientConfig>(
            "UdpClientConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for UdpClientConfig {
    const NAME: &'static str = "UdpClientConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.common)?;
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
        if let Some(v) = self.common.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.common.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
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

    fn new() -> UdpClientConfig {
        UdpClientConfig::new()
    }

    fn clear(&mut self) {
        self.common.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static UdpClientConfig {
        static instance: UdpClientConfig = UdpClientConfig {
            common: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for UdpClientConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("UdpClientConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for UdpClientConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for UdpClientConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.outflow.v1.NullClientConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct NullClientConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.NullClientConfig.common)
    pub common: ::protobuf::MessageField<CommonWireClientConfig>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.NullClientConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a NullClientConfig {
    fn default() -> &'a NullClientConfig {
        <NullClientConfig as ::protobuf::Message>::default_instance()
    }
}

impl NullClientConfig {
    pub fn new() -> NullClientConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, CommonWireClientConfig>(
            "common",
            |m: &NullClientConfig| { &m.common },
            |m: &mut NullClientConfig| { &mut m.common },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<NullClientConfig>(
            "NullClientConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for NullClientConfig {
    const NAME: &'static str = "NullClientConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.common)?;
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
        if let Some(v) = self.common.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.common.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
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

    fn new() -> NullClientConfig {
        NullClientConfig::new()
    }

    fn clear(&mut self) {
        self.common.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static NullClientConfig {
        static instance: NullClientConfig = NullClientConfig {
            common: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for NullClientConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("NullClientConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for NullClientConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for NullClientConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.outflow.v1.TcpClientConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct TcpClientConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.TcpClientConfig.common)
    pub common: ::protobuf::MessageField<CommonWireClientConfig>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.TcpClientConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a TcpClientConfig {
    fn default() -> &'a TcpClientConfig {
        <TcpClientConfig as ::protobuf::Message>::default_instance()
    }
}

impl TcpClientConfig {
    pub fn new() -> TcpClientConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, CommonWireClientConfig>(
            "common",
            |m: &TcpClientConfig| { &m.common },
            |m: &mut TcpClientConfig| { &mut m.common },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<TcpClientConfig>(
            "TcpClientConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for TcpClientConfig {
    const NAME: &'static str = "TcpClientConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.common)?;
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
        if let Some(v) = self.common.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.common.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
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

    fn new() -> TcpClientConfig {
        TcpClientConfig::new()
    }

    fn clear(&mut self) {
        self.common.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static TcpClientConfig {
        static instance: TcpClientConfig = TcpClientConfig {
            common: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for TcpClientConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("TcpClientConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for TcpClientConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for TcpClientConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n\"pulse/config/outflow/v1/wire.proto\x12\x17pulse.config.outflow.v1\
    \x1a#pulse/config/common/v1/common.proto\x1a*pulse/config/outflow/v1/que\
    ue_policy.proto\x1a\x1egoogle/protobuf/duration.proto\x1a\x17validate/va\
    lidate.proto\"\x8c\x03\n\x16CommonWireClientConfig\x12\x17\n\x07send_to\
    \x18\x01\x20\x01(\tR\x06sendTo\x12J\n\x08protocol\x18\x02\x20\x01(\x0b2$\
    .pulse.config.common.v1.WireProtocolR\x08protocolB\x08\xfaB\x05\x8a\x01\
    \x02\x10\x01\x12G\n\x0cqueue_policy\x18\x03\x20\x01(\x0b2$.pulse.config.\
    outflow.v1.QueuePolicyR\x0bqueuePolicy\x12+\n\x0fbatch_max_bytes\x18\x04\
    \x20\x01(\x04H\0R\rbatchMaxBytes\x88\x01\x01\x12H\n\rwrite_timeout\x18\
    \x05\x20\x01(\x0b2\x19.google.protobuf.DurationR\x0cwriteTimeoutB\x08\
    \xfaB\x05\xaa\x01\x02*\0\x12'\n\rmax_in_flight\x18\x06\x20\x01(\x04H\x01\
    R\x0bmaxInFlight\x88\x01\x01B\x12\n\x10_batch_max_bytesB\x10\n\x0e_max_i\
    n_flight\"e\n\x10UnixClientConfig\x12Q\n\x06common\x18\x01\x20\x01(\x0b2\
    /.pulse.config.outflow.v1.CommonWireClientConfigR\x06commonB\x08\xfaB\
    \x05\x8a\x01\x02\x10\x01\"d\n\x0fUdpClientConfig\x12Q\n\x06common\x18\
    \x01\x20\x01(\x0b2/.pulse.config.outflow.v1.CommonWireClientConfigR\x06c\
    ommonB\x08\xfaB\x05\x8a\x01\x02\x10\x01\"[\n\x10NullClientConfig\x12G\n\
    \x06common\x18\x01\x20\x01(\x0b2/.pulse.config.outflow.v1.CommonWireClie\
    ntConfigR\x06common\"d\n\x0fTcpClientConfig\x12Q\n\x06common\x18\x01\x20\
    \x01(\x0b2/.pulse.config.outflow.v1.CommonWireClientConfigR\x06commonB\
    \x08\xfaB\x05\x8a\x01\x02\x10\x01b\x06proto3\
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
            let mut deps = ::std::vec::Vec::with_capacity(4);
            deps.push(super::common::file_descriptor().clone());
            deps.push(super::queue_policy::file_descriptor().clone());
            deps.push(::protobuf::well_known_types::duration::file_descriptor().clone());
            deps.push(super::validate::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(5);
            messages.push(CommonWireClientConfig::generated_message_descriptor_data());
            messages.push(UnixClientConfig::generated_message_descriptor_data());
            messages.push(UdpClientConfig::generated_message_descriptor_data());
            messages.push(NullClientConfig::generated_message_descriptor_data());
            messages.push(TcpClientConfig::generated_message_descriptor_data());
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