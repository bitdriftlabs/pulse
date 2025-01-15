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

//! Generated file from `pulse/config/common/v1/retry.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.common.v1.AwsSqsRetryOffloadQueue)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct AwsSqsRetryOffloadQueue {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.common.v1.AwsSqsRetryOffloadQueue.queue_name)
    pub queue_name: ::protobuf::Chars,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.common.v1.AwsSqsRetryOffloadQueue.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a AwsSqsRetryOffloadQueue {
    fn default() -> &'a AwsSqsRetryOffloadQueue {
        <AwsSqsRetryOffloadQueue as ::protobuf::Message>::default_instance()
    }
}

impl AwsSqsRetryOffloadQueue {
    pub fn new() -> AwsSqsRetryOffloadQueue {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "queue_name",
            |m: &AwsSqsRetryOffloadQueue| { &m.queue_name },
            |m: &mut AwsSqsRetryOffloadQueue| { &mut m.queue_name },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<AwsSqsRetryOffloadQueue>(
            "AwsSqsRetryOffloadQueue",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for AwsSqsRetryOffloadQueue {
    const NAME: &'static str = "AwsSqsRetryOffloadQueue";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.queue_name = is.read_tokio_chars()?;
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
        if !self.queue_name.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.queue_name);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if !self.queue_name.is_empty() {
            os.write_string(1, &self.queue_name)?;
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

    fn new() -> AwsSqsRetryOffloadQueue {
        AwsSqsRetryOffloadQueue::new()
    }

    fn clear(&mut self) {
        self.queue_name.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static AwsSqsRetryOffloadQueue {
        static instance: AwsSqsRetryOffloadQueue = AwsSqsRetryOffloadQueue {
            queue_name: ::protobuf::Chars::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for AwsSqsRetryOffloadQueue {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("AwsSqsRetryOffloadQueue").unwrap()).clone()
    }
}

impl ::std::fmt::Display for AwsSqsRetryOffloadQueue {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for AwsSqsRetryOffloadQueue {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.common.v1.LoopbackForTestOffloadQueue)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct LoopbackForTestOffloadQueue {
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.common.v1.LoopbackForTestOffloadQueue.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a LoopbackForTestOffloadQueue {
    fn default() -> &'a LoopbackForTestOffloadQueue {
        <LoopbackForTestOffloadQueue as ::protobuf::Message>::default_instance()
    }
}

impl LoopbackForTestOffloadQueue {
    pub fn new() -> LoopbackForTestOffloadQueue {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(0);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<LoopbackForTestOffloadQueue>(
            "LoopbackForTestOffloadQueue",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for LoopbackForTestOffloadQueue {
    const NAME: &'static str = "LoopbackForTestOffloadQueue";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
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
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        os.write_unknown_fields(self.special_fields.unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn special_fields(&self) -> &::protobuf::SpecialFields {
        &self.special_fields
    }

    fn mut_special_fields(&mut self) -> &mut ::protobuf::SpecialFields {
        &mut self.special_fields
    }

    fn new() -> LoopbackForTestOffloadQueue {
        LoopbackForTestOffloadQueue::new()
    }

    fn clear(&mut self) {
        self.special_fields.clear();
    }

    fn default_instance() -> &'static LoopbackForTestOffloadQueue {
        static instance: LoopbackForTestOffloadQueue = LoopbackForTestOffloadQueue {
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for LoopbackForTestOffloadQueue {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("LoopbackForTestOffloadQueue").unwrap()).clone()
    }
}

impl ::std::fmt::Display for LoopbackForTestOffloadQueue {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for LoopbackForTestOffloadQueue {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.config.common.v1.RetryOffloadQueue)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct RetryOffloadQueue {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryOffloadQueue.max_send_attempts)
    pub max_send_attempts: ::std::option::Option<u32>,
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryOffloadQueue.backoff)
    pub backoff: ::protobuf::MessageField<::protobuf::well_known_types::duration::Duration>,
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryOffloadQueue.window)
    pub window: ::protobuf::MessageField<::protobuf::well_known_types::duration::Duration>,
    // message oneof groups
    pub queue_type: ::std::option::Option<retry_offload_queue::Queue_type>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.common.v1.RetryOffloadQueue.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a RetryOffloadQueue {
    fn default() -> &'a RetryOffloadQueue {
        <RetryOffloadQueue as ::protobuf::Message>::default_instance()
    }
}

impl RetryOffloadQueue {
    pub fn new() -> RetryOffloadQueue {
        ::std::default::Default::default()
    }

    // .pulse.config.common.v1.AwsSqsRetryOffloadQueue aws_sqs = 4;

    pub fn aws_sqs(&self) -> &AwsSqsRetryOffloadQueue {
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(ref v)) => v,
            _ => <AwsSqsRetryOffloadQueue as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_aws_sqs(&mut self) {
        self.queue_type = ::std::option::Option::None;
    }

    pub fn has_aws_sqs(&self) -> bool {
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_aws_sqs(&mut self, v: AwsSqsRetryOffloadQueue) {
        self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(v))
    }

    // Mutable pointer to the field.
    pub fn mut_aws_sqs(&mut self) -> &mut AwsSqsRetryOffloadQueue {
        if let ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(_)) = self.queue_type {
        } else {
            self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(AwsSqsRetryOffloadQueue::new()));
        }
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_aws_sqs(&mut self) -> AwsSqsRetryOffloadQueue {
        if self.has_aws_sqs() {
            match self.queue_type.take() {
                ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(v)) => v,
                _ => panic!(),
            }
        } else {
            AwsSqsRetryOffloadQueue::new()
        }
    }

    // .pulse.config.common.v1.LoopbackForTestOffloadQueue loopback_for_test = 5;

    pub fn loopback_for_test(&self) -> &LoopbackForTestOffloadQueue {
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(ref v)) => v,
            _ => <LoopbackForTestOffloadQueue as ::protobuf::Message>::default_instance(),
        }
    }

    pub fn clear_loopback_for_test(&mut self) {
        self.queue_type = ::std::option::Option::None;
    }

    pub fn has_loopback_for_test(&self) -> bool {
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(..)) => true,
            _ => false,
        }
    }

    // Param is passed by value, moved
    pub fn set_loopback_for_test(&mut self, v: LoopbackForTestOffloadQueue) {
        self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(v))
    }

    // Mutable pointer to the field.
    pub fn mut_loopback_for_test(&mut self) -> &mut LoopbackForTestOffloadQueue {
        if let ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(_)) = self.queue_type {
        } else {
            self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(LoopbackForTestOffloadQueue::new()));
        }
        match self.queue_type {
            ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(ref mut v)) => v,
            _ => panic!(),
        }
    }

    // Take field
    pub fn take_loopback_for_test(&mut self) -> LoopbackForTestOffloadQueue {
        if self.has_loopback_for_test() {
            match self.queue_type.take() {
                ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(v)) => v,
                _ => panic!(),
            }
        } else {
            LoopbackForTestOffloadQueue::new()
        }
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(5);
        let mut oneofs = ::std::vec::Vec::with_capacity(1);
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "max_send_attempts",
            |m: &RetryOffloadQueue| { &m.max_send_attempts },
            |m: &mut RetryOffloadQueue| { &mut m.max_send_attempts },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ::protobuf::well_known_types::duration::Duration>(
            "backoff",
            |m: &RetryOffloadQueue| { &m.backoff },
            |m: &mut RetryOffloadQueue| { &mut m.backoff },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ::protobuf::well_known_types::duration::Duration>(
            "window",
            |m: &RetryOffloadQueue| { &m.window },
            |m: &mut RetryOffloadQueue| { &mut m.window },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, AwsSqsRetryOffloadQueue>(
            "aws_sqs",
            RetryOffloadQueue::has_aws_sqs,
            RetryOffloadQueue::aws_sqs,
            RetryOffloadQueue::mut_aws_sqs,
            RetryOffloadQueue::set_aws_sqs,
        ));
        fields.push(::protobuf::reflect::rt::v2::make_oneof_message_has_get_mut_set_accessor::<_, LoopbackForTestOffloadQueue>(
            "loopback_for_test",
            RetryOffloadQueue::has_loopback_for_test,
            RetryOffloadQueue::loopback_for_test,
            RetryOffloadQueue::mut_loopback_for_test,
            RetryOffloadQueue::set_loopback_for_test,
        ));
        oneofs.push(retry_offload_queue::Queue_type::generated_oneof_descriptor_data());
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<RetryOffloadQueue>(
            "RetryOffloadQueue",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for RetryOffloadQueue {
    const NAME: &'static str = "RetryOffloadQueue";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                8 => {
                    self.max_send_attempts = ::std::option::Option::Some(is.read_uint32()?);
                },
                18 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.backoff)?;
                },
                26 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.window)?;
                },
                34 => {
                    self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::AwsSqs(is.read_message()?));
                },
                42 => {
                    self.queue_type = ::std::option::Option::Some(retry_offload_queue::Queue_type::LoopbackForTest(is.read_message()?));
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
        if let Some(v) = self.max_send_attempts {
            my_size += ::protobuf::rt::uint32_size(1, v);
        }
        if let Some(v) = self.backoff.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let Some(v) = self.window.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let ::std::option::Option::Some(ref v) = self.queue_type {
            match v {
                &retry_offload_queue::Queue_type::AwsSqs(ref v) => {
                    let len = v.compute_size();
                    my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
                },
                &retry_offload_queue::Queue_type::LoopbackForTest(ref v) => {
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
        if let Some(v) = self.max_send_attempts {
            os.write_uint32(1, v)?;
        }
        if let Some(v) = self.backoff.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
        }
        if let Some(v) = self.window.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(3, v, os)?;
        }
        if let ::std::option::Option::Some(ref v) = self.queue_type {
            match v {
                &retry_offload_queue::Queue_type::AwsSqs(ref v) => {
                    ::protobuf::rt::write_message_field_with_cached_size(4, v, os)?;
                },
                &retry_offload_queue::Queue_type::LoopbackForTest(ref v) => {
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

    fn new() -> RetryOffloadQueue {
        RetryOffloadQueue::new()
    }

    fn clear(&mut self) {
        self.max_send_attempts = ::std::option::Option::None;
        self.backoff.clear();
        self.window.clear();
        self.queue_type = ::std::option::Option::None;
        self.queue_type = ::std::option::Option::None;
        self.special_fields.clear();
    }

    fn default_instance() -> &'static RetryOffloadQueue {
        static instance: RetryOffloadQueue = RetryOffloadQueue {
            max_send_attempts: ::std::option::Option::None,
            backoff: ::protobuf::MessageField::none(),
            window: ::protobuf::MessageField::none(),
            queue_type: ::std::option::Option::None,
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for RetryOffloadQueue {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("RetryOffloadQueue").unwrap()).clone()
    }
}

impl ::std::fmt::Display for RetryOffloadQueue {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for RetryOffloadQueue {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

/// Nested message and enums of message `RetryOffloadQueue`
pub mod retry_offload_queue {

    #[derive(Clone,PartialEq,Debug)]
    // @@protoc_insertion_point(oneof:pulse.config.common.v1.RetryOffloadQueue.queue_type)
    pub enum Queue_type {
        // @@protoc_insertion_point(oneof_field:pulse.config.common.v1.RetryOffloadQueue.aws_sqs)
        AwsSqs(super::AwsSqsRetryOffloadQueue),
        // @@protoc_insertion_point(oneof_field:pulse.config.common.v1.RetryOffloadQueue.loopback_for_test)
        LoopbackForTest(super::LoopbackForTestOffloadQueue),
    }

    impl ::protobuf::Oneof for Queue_type {
    }

    impl ::protobuf::OneofFull for Queue_type {
        fn descriptor() -> ::protobuf::reflect::OneofDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::OneofDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| <super::RetryOffloadQueue as ::protobuf::MessageFull>::descriptor().oneof_by_name("queue_type").unwrap()).clone()
        }
    }

    impl Queue_type {
        pub(in super) fn generated_oneof_descriptor_data() -> ::protobuf::reflect::GeneratedOneofDescriptorData {
            ::protobuf::reflect::GeneratedOneofDescriptorData::new::<Queue_type>("queue_type")
        }
    }
}

// @@protoc_insertion_point(message:pulse.config.common.v1.RetryPolicy)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct RetryPolicy {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryPolicy.budget)
    pub budget: ::std::option::Option<f64>,
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryPolicy.max_retries)
    pub max_retries: ::std::option::Option<u32>,
    // @@protoc_insertion_point(field:pulse.config.common.v1.RetryPolicy.offload_queue)
    pub offload_queue: ::protobuf::MessageField<RetryOffloadQueue>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.common.v1.RetryPolicy.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a RetryPolicy {
    fn default() -> &'a RetryPolicy {
        <RetryPolicy as ::protobuf::Message>::default_instance()
    }
}

impl RetryPolicy {
    pub fn new() -> RetryPolicy {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(3);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "budget",
            |m: &RetryPolicy| { &m.budget },
            |m: &mut RetryPolicy| { &mut m.budget },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "max_retries",
            |m: &RetryPolicy| { &m.max_retries },
            |m: &mut RetryPolicy| { &mut m.max_retries },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, RetryOffloadQueue>(
            "offload_queue",
            |m: &RetryPolicy| { &m.offload_queue },
            |m: &mut RetryPolicy| { &mut m.offload_queue },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<RetryPolicy>(
            "RetryPolicy",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for RetryPolicy {
    const NAME: &'static str = "RetryPolicy";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                9 => {
                    self.budget = ::std::option::Option::Some(is.read_double()?);
                },
                16 => {
                    self.max_retries = ::std::option::Option::Some(is.read_uint32()?);
                },
                26 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.offload_queue)?;
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
        if let Some(v) = self.budget {
            my_size += 1 + 8;
        }
        if let Some(v) = self.max_retries {
            my_size += ::protobuf::rt::uint32_size(2, v);
        }
        if let Some(v) = self.offload_queue.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.budget {
            os.write_double(1, v)?;
        }
        if let Some(v) = self.max_retries {
            os.write_uint32(2, v)?;
        }
        if let Some(v) = self.offload_queue.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(3, v, os)?;
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

    fn new() -> RetryPolicy {
        RetryPolicy::new()
    }

    fn clear(&mut self) {
        self.budget = ::std::option::Option::None;
        self.max_retries = ::std::option::Option::None;
        self.offload_queue.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static RetryPolicy {
        static instance: RetryPolicy = RetryPolicy {
            budget: ::std::option::Option::None,
            max_retries: ::std::option::Option::None,
            offload_queue: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for RetryPolicy {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("RetryPolicy").unwrap()).clone()
    }
}

impl ::std::fmt::Display for RetryPolicy {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for RetryPolicy {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n\"pulse/config/common/v1/retry.proto\x12\x16pulse.config.common.v1\x1a\
    \x1egoogle/protobuf/duration.proto\x1a\x17validate/validate.proto\"A\n\
    \x17AwsSqsRetryOffloadQueue\x12&\n\nqueue_name\x18\x01\x20\x01(\tR\tqueu\
    eNameB\x07\xfaB\x04r\x02\x10\x01\"\x1d\n\x1bLoopbackForTestOffloadQueue\
    \"\x98\x03\n\x11RetryOffloadQueue\x12/\n\x11max_send_attempts\x18\x01\
    \x20\x01(\rH\x01R\x0fmaxSendAttempts\x88\x01\x01\x12=\n\x07backoff\x18\
    \x02\x20\x01(\x0b2\x19.google.protobuf.DurationR\x07backoffB\x08\xfaB\
    \x05\xaa\x01\x02*\0\x12;\n\x06window\x18\x03\x20\x01(\x0b2\x19.google.pr\
    otobuf.DurationR\x06windowB\x08\xfaB\x05\xaa\x01\x02*\0\x12J\n\x07aws_sq\
    s\x18\x04\x20\x01(\x0b2/.pulse.config.common.v1.AwsSqsRetryOffloadQueueH\
    \0R\x06awsSqs\x12a\n\x11loopback_for_test\x18\x05\x20\x01(\x0b23.pulse.c\
    onfig.common.v1.LoopbackForTestOffloadQueueH\0R\x0floopbackForTestB\x11\
    \n\nqueue_type\x12\x03\xf8B\x01B\x14\n\x12_max_send_attempts\"\xbb\x01\n\
    \x0bRetryPolicy\x12\x1b\n\x06budget\x18\x01\x20\x01(\x01H\0R\x06budget\
    \x88\x01\x01\x12$\n\x0bmax_retries\x18\x02\x20\x01(\rH\x01R\nmaxRetries\
    \x88\x01\x01\x12N\n\roffload_queue\x18\x03\x20\x01(\x0b2).pulse.config.c\
    ommon.v1.RetryOffloadQueueR\x0coffloadQueueB\t\n\x07_budgetB\x0e\n\x0c_m\
    ax_retriesb\x06proto3\
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
            let mut messages = ::std::vec::Vec::with_capacity(4);
            messages.push(AwsSqsRetryOffloadQueue::generated_message_descriptor_data());
            messages.push(LoopbackForTestOffloadQueue::generated_message_descriptor_data());
            messages.push(RetryOffloadQueue::generated_message_descriptor_data());
            messages.push(RetryPolicy::generated_message_descriptor_data());
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
