// proto - bitdrift's client/server API definitions
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code and APIs are governed by a source available license that can be found in
// the LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

// This file is generated by rust-protobuf 4.0.0-alpha.0. Do not edit
// .proto file is parsed by protoc 29.2
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

//! Generated file from `pulse/config/outflow/v1/queue_policy.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.outflow.v1.QueuePolicy)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct QueuePolicy {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.QueuePolicy.queue_max_bytes)
    pub queue_max_bytes: ::std::option::Option<u64>,
    // @@protoc_insertion_point(field:pulse.config.outflow.v1.QueuePolicy.batch_fill_wait)
    pub batch_fill_wait: ::protobuf::MessageField<::protobuf::well_known_types::duration::Duration>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.outflow.v1.QueuePolicy.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a QueuePolicy {
    fn default() -> &'a QueuePolicy {
        <QueuePolicy as ::protobuf::Message>::default_instance()
    }
}

impl QueuePolicy {
    pub fn new() -> QueuePolicy {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(2);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "queue_max_bytes",
            |m: &QueuePolicy| { &m.queue_max_bytes },
            |m: &mut QueuePolicy| { &mut m.queue_max_bytes },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ::protobuf::well_known_types::duration::Duration>(
            "batch_fill_wait",
            |m: &QueuePolicy| { &m.batch_fill_wait },
            |m: &mut QueuePolicy| { &mut m.batch_fill_wait },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<QueuePolicy>(
            "QueuePolicy",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for QueuePolicy {
    const NAME: &'static str = "QueuePolicy";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                8 => {
                    self.queue_max_bytes = ::std::option::Option::Some(is.read_uint64()?);
                },
                18 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.batch_fill_wait)?;
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
        if let Some(v) = self.queue_max_bytes {
            my_size += ::protobuf::rt::uint64_size(1, v);
        }
        if let Some(v) = self.batch_fill_wait.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.queue_max_bytes {
            os.write_uint64(1, v)?;
        }
        if let Some(v) = self.batch_fill_wait.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
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

    fn new() -> QueuePolicy {
        QueuePolicy::new()
    }

    fn clear(&mut self) {
        self.queue_max_bytes = ::std::option::Option::None;
        self.batch_fill_wait.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static QueuePolicy {
        static instance: QueuePolicy = QueuePolicy {
            queue_max_bytes: ::std::option::Option::None,
            batch_fill_wait: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for QueuePolicy {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("QueuePolicy").unwrap()).clone()
    }
}

impl ::std::fmt::Display for QueuePolicy {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for QueuePolicy {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n*pulse/config/outflow/v1/queue_policy.proto\x12\x17pulse.config.outflo\
    w.v1\x1a\x1egoogle/protobuf/duration.proto\x1a\x17validate/validate.prot\
    o\"\x9b\x01\n\x0bQueuePolicy\x12+\n\x0fqueue_max_bytes\x18\x01\x20\x01(\
    \x04H\0R\rqueueMaxBytes\x88\x01\x01\x12K\n\x0fbatch_fill_wait\x18\x02\
    \x20\x01(\x0b2\x19.google.protobuf.DurationR\rbatchFillWaitB\x08\xfaB\
    \x05\xaa\x01\x02*\0B\x12\n\x10_queue_max_bytesb\x06proto3\
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
            let mut deps = ::std::vec::Vec::with_capacity(2);
            deps.push(::protobuf::well_known_types::duration::file_descriptor().clone());
            deps.push(super::validate::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(1);
            messages.push(QueuePolicy::generated_message_descriptor_data());
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
