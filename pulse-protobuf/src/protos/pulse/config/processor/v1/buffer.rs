// proto - bitdrift's client/server API definitions
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code and APIs are governed by a source available license that can be found in
// the LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

// This file is generated by rust-protobuf 4.0.0-alpha.0. Do not edit
// .proto file is parsed by protoc 29.3
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

//! Generated file from `pulse/config/processor/v1/buffer.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.processor.v1.BufferConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct BufferConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.processor.v1.BufferConfig.max_buffered_metrics)
    pub max_buffered_metrics: u32,
    // @@protoc_insertion_point(field:pulse.config.processor.v1.BufferConfig.num_consumers)
    pub num_consumers: u32,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.processor.v1.BufferConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a BufferConfig {
    fn default() -> &'a BufferConfig {
        <BufferConfig as ::protobuf::Message>::default_instance()
    }
}

impl BufferConfig {
    pub fn new() -> BufferConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(2);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "max_buffered_metrics",
            |m: &BufferConfig| { &m.max_buffered_metrics },
            |m: &mut BufferConfig| { &mut m.max_buffered_metrics },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "num_consumers",
            |m: &BufferConfig| { &m.num_consumers },
            |m: &mut BufferConfig| { &mut m.num_consumers },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<BufferConfig>(
            "BufferConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for BufferConfig {
    const NAME: &'static str = "BufferConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                8 => {
                    self.max_buffered_metrics = is.read_uint32()?;
                },
                16 => {
                    self.num_consumers = is.read_uint32()?;
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
        if self.max_buffered_metrics != 0 {
            my_size += ::protobuf::rt::uint32_size(1, self.max_buffered_metrics);
        }
        if self.num_consumers != 0 {
            my_size += ::protobuf::rt::uint32_size(2, self.num_consumers);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if self.max_buffered_metrics != 0 {
            os.write_uint32(1, self.max_buffered_metrics)?;
        }
        if self.num_consumers != 0 {
            os.write_uint32(2, self.num_consumers)?;
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

    fn new() -> BufferConfig {
        BufferConfig::new()
    }

    fn clear(&mut self) {
        self.max_buffered_metrics = 0;
        self.num_consumers = 0;
        self.special_fields.clear();
    }

    fn default_instance() -> &'static BufferConfig {
        static instance: BufferConfig = BufferConfig {
            max_buffered_metrics: 0,
            num_consumers: 0,
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for BufferConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("BufferConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for BufferConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for BufferConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n&pulse/config/processor/v1/buffer.proto\x12\x19pulse.config.processor.\
    v1\x1a\x17validate/validate.proto\"w\n\x0cBufferConfig\x129\n\x14max_buf\
    fered_metrics\x18\x01\x20\x01(\rR\x12maxBufferedMetricsB\x07\xfaB\x04*\
    \x02\x20\0\x12,\n\rnum_consumers\x18\x02\x20\x01(\rR\x0cnumConsumersB\
    \x07\xfaB\x04*\x02\x20\0b\x06proto3\
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
            let mut deps = ::std::vec::Vec::with_capacity(1);
            deps.push(super::validate::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(1);
            messages.push(BufferConfig::generated_message_descriptor_data());
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
