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

//! Generated file from `pulse/internode/v1/internode.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.internode.v1.PeersComparisonRequest)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct PeersComparisonRequest {
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.PeersComparisonRequest.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a PeersComparisonRequest {
    fn default() -> &'a PeersComparisonRequest {
        <PeersComparisonRequest as ::protobuf::Message>::default_instance()
    }
}

impl PeersComparisonRequest {
    pub fn new() -> PeersComparisonRequest {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(0);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<PeersComparisonRequest>(
            "PeersComparisonRequest",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for PeersComparisonRequest {
    const NAME: &'static str = "PeersComparisonRequest";

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

    fn new() -> PeersComparisonRequest {
        PeersComparisonRequest::new()
    }

    fn clear(&mut self) {
        self.special_fields.clear();
    }

    fn default_instance() -> &'static PeersComparisonRequest {
        static instance: PeersComparisonRequest = PeersComparisonRequest {
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for PeersComparisonRequest {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("PeersComparisonRequest").unwrap()).clone()
    }
}

impl ::std::fmt::Display for PeersComparisonRequest {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for PeersComparisonRequest {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.internode.v1.PeersComparisonResponse)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct PeersComparisonResponse {
    // message fields
    // @@protoc_insertion_point(field:pulse.internode.v1.PeersComparisonResponse.peer)
    pub peer: ::std::vec::Vec<::protobuf::Chars>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.PeersComparisonResponse.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a PeersComparisonResponse {
    fn default() -> &'a PeersComparisonResponse {
        <PeersComparisonResponse as ::protobuf::Message>::default_instance()
    }
}

impl PeersComparisonResponse {
    pub fn new() -> PeersComparisonResponse {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_vec_simpler_accessor::<_, _>(
            "peer",
            |m: &PeersComparisonResponse| { &m.peer },
            |m: &mut PeersComparisonResponse| { &mut m.peer },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<PeersComparisonResponse>(
            "PeersComparisonResponse",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for PeersComparisonResponse {
    const NAME: &'static str = "PeersComparisonResponse";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.peer.push(is.read_tokio_chars()?);
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
        for value in &self.peer {
            my_size += ::protobuf::rt::string_size(1, &value);
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        for v in &self.peer {
            os.write_string(1, &v)?;
        };
        os.write_unknown_fields(self.special_fields.unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn special_fields(&self) -> &::protobuf::SpecialFields {
        &self.special_fields
    }

    fn mut_special_fields(&mut self) -> &mut ::protobuf::SpecialFields {
        &mut self.special_fields
    }

    fn new() -> PeersComparisonResponse {
        PeersComparisonResponse::new()
    }

    fn clear(&mut self) {
        self.peer.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static PeersComparisonResponse {
        static instance: PeersComparisonResponse = PeersComparisonResponse {
            peer: ::std::vec::Vec::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for PeersComparisonResponse {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("PeersComparisonResponse").unwrap()).clone()
    }
}

impl ::std::fmt::Display for PeersComparisonResponse {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for PeersComparisonResponse {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.internode.v1.LastElidedTimestampRequest)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct LastElidedTimestampRequest {
    // message fields
    // @@protoc_insertion_point(field:pulse.internode.v1.LastElidedTimestampRequest.metric)
    pub metric: ::protobuf::Chars,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.LastElidedTimestampRequest.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a LastElidedTimestampRequest {
    fn default() -> &'a LastElidedTimestampRequest {
        <LastElidedTimestampRequest as ::protobuf::Message>::default_instance()
    }
}

impl LastElidedTimestampRequest {
    pub fn new() -> LastElidedTimestampRequest {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "metric",
            |m: &LastElidedTimestampRequest| { &m.metric },
            |m: &mut LastElidedTimestampRequest| { &mut m.metric },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<LastElidedTimestampRequest>(
            "LastElidedTimestampRequest",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for LastElidedTimestampRequest {
    const NAME: &'static str = "LastElidedTimestampRequest";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.metric = is.read_tokio_chars()?;
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
        if !self.metric.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.metric);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if !self.metric.is_empty() {
            os.write_string(1, &self.metric)?;
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

    fn new() -> LastElidedTimestampRequest {
        LastElidedTimestampRequest::new()
    }

    fn clear(&mut self) {
        self.metric.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static LastElidedTimestampRequest {
        static instance: LastElidedTimestampRequest = LastElidedTimestampRequest {
            metric: ::protobuf::Chars::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for LastElidedTimestampRequest {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("LastElidedTimestampRequest").unwrap()).clone()
    }
}

impl ::std::fmt::Display for LastElidedTimestampRequest {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for LastElidedTimestampRequest {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.internode.v1.LastElidedTimestampResponse)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct LastElidedTimestampResponse {
    // message fields
    // @@protoc_insertion_point(field:pulse.internode.v1.LastElidedTimestampResponse.timestamp)
    pub timestamp: u64,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.LastElidedTimestampResponse.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a LastElidedTimestampResponse {
    fn default() -> &'a LastElidedTimestampResponse {
        <LastElidedTimestampResponse as ::protobuf::Message>::default_instance()
    }
}

impl LastElidedTimestampResponse {
    pub fn new() -> LastElidedTimestampResponse {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "timestamp",
            |m: &LastElidedTimestampResponse| { &m.timestamp },
            |m: &mut LastElidedTimestampResponse| { &mut m.timestamp },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<LastElidedTimestampResponse>(
            "LastElidedTimestampResponse",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for LastElidedTimestampResponse {
    const NAME: &'static str = "LastElidedTimestampResponse";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                8 => {
                    self.timestamp = is.read_uint64()?;
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
        if self.timestamp != 0 {
            my_size += ::protobuf::rt::uint64_size(1, self.timestamp);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if self.timestamp != 0 {
            os.write_uint64(1, self.timestamp)?;
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

    fn new() -> LastElidedTimestampResponse {
        LastElidedTimestampResponse::new()
    }

    fn clear(&mut self) {
        self.timestamp = 0;
        self.special_fields.clear();
    }

    fn default_instance() -> &'static LastElidedTimestampResponse {
        static instance: LastElidedTimestampResponse = LastElidedTimestampResponse {
            timestamp: 0,
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for LastElidedTimestampResponse {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("LastElidedTimestampResponse").unwrap()).clone()
    }
}

impl ::std::fmt::Display for LastElidedTimestampResponse {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for LastElidedTimestampResponse {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.internode.v1.InternodeMetricsRequest)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct InternodeMetricsRequest {
    // message fields
    // @@protoc_insertion_point(field:pulse.internode.v1.InternodeMetricsRequest.metrics)
    pub metrics: ::std::vec::Vec<super::metric::Metric>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.InternodeMetricsRequest.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a InternodeMetricsRequest {
    fn default() -> &'a InternodeMetricsRequest {
        <InternodeMetricsRequest as ::protobuf::Message>::default_instance()
    }
}

impl InternodeMetricsRequest {
    pub fn new() -> InternodeMetricsRequest {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_vec_simpler_accessor::<_, _>(
            "metrics",
            |m: &InternodeMetricsRequest| { &m.metrics },
            |m: &mut InternodeMetricsRequest| { &mut m.metrics },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<InternodeMetricsRequest>(
            "InternodeMetricsRequest",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for InternodeMetricsRequest {
    const NAME: &'static str = "InternodeMetricsRequest";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.metrics.push(is.read_message()?);
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
        for value in &self.metrics {
            let len = value.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        for v in &self.metrics {
            ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
        };
        os.write_unknown_fields(self.special_fields.unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn special_fields(&self) -> &::protobuf::SpecialFields {
        &self.special_fields
    }

    fn mut_special_fields(&mut self) -> &mut ::protobuf::SpecialFields {
        &mut self.special_fields
    }

    fn new() -> InternodeMetricsRequest {
        InternodeMetricsRequest::new()
    }

    fn clear(&mut self) {
        self.metrics.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static InternodeMetricsRequest {
        static instance: InternodeMetricsRequest = InternodeMetricsRequest {
            metrics: ::std::vec::Vec::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for InternodeMetricsRequest {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("InternodeMetricsRequest").unwrap()).clone()
    }
}

impl ::std::fmt::Display for InternodeMetricsRequest {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for InternodeMetricsRequest {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.internode.v1.InternodeMetricsResponse)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct InternodeMetricsResponse {
    // special fields
    // @@protoc_insertion_point(special_field:pulse.internode.v1.InternodeMetricsResponse.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a InternodeMetricsResponse {
    fn default() -> &'a InternodeMetricsResponse {
        <InternodeMetricsResponse as ::protobuf::Message>::default_instance()
    }
}

impl InternodeMetricsResponse {
    pub fn new() -> InternodeMetricsResponse {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(0);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<InternodeMetricsResponse>(
            "InternodeMetricsResponse",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for InternodeMetricsResponse {
    const NAME: &'static str = "InternodeMetricsResponse";

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

    fn new() -> InternodeMetricsResponse {
        InternodeMetricsResponse::new()
    }

    fn clear(&mut self) {
        self.special_fields.clear();
    }

    fn default_instance() -> &'static InternodeMetricsResponse {
        static instance: InternodeMetricsResponse = InternodeMetricsResponse {
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for InternodeMetricsResponse {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("InternodeMetricsResponse").unwrap()).clone()
    }
}

impl ::std::fmt::Display for InternodeMetricsResponse {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for InternodeMetricsResponse {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n\"pulse/internode/v1/internode.proto\x12\x12pulse.internode.v1\x1a\x1f\
    pulse/internode/v1/metric.proto\"\x18\n\x16PeersComparisonRequest\"-\n\
    \x17PeersComparisonResponse\x12\x12\n\x04peer\x18\x01\x20\x03(\tR\x04pee\
    r\"4\n\x1aLastElidedTimestampRequest\x12\x16\n\x06metric\x18\x01\x20\x01\
    (\tR\x06metric\";\n\x1bLastElidedTimestampResponse\x12\x1c\n\ttimestamp\
    \x18\x01\x20\x01(\x04R\ttimestamp\"O\n\x17InternodeMetricsRequest\x124\n\
    \x07metrics\x18\x01\x20\x03(\x0b2\x1a.pulse.internode.v1.MetricR\x07metr\
    ics\"\x1a\n\x18InternodeMetricsResponse2\xe1\x02\n\tInternode\x12m\n\x10\
    InternodeMetrics\x12+.pulse.internode.v1.InternodeMetricsRequest\x1a,.pu\
    lse.internode.v1.InternodeMetricsResponse\x12m\n\x12GetPeersComparison\
    \x12*.pulse.internode.v1.PeersComparisonRequest\x1a+.pulse.internode.v1.\
    PeersComparisonResponse\x12v\n\x13LastElidedTimestamp\x12..pulse.interno\
    de.v1.LastElidedTimestampRequest\x1a/.pulse.internode.v1.LastElidedTimes\
    tampResponseb\x06proto3\
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
            deps.push(super::metric::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(6);
            messages.push(PeersComparisonRequest::generated_message_descriptor_data());
            messages.push(PeersComparisonResponse::generated_message_descriptor_data());
            messages.push(LastElidedTimestampRequest::generated_message_descriptor_data());
            messages.push(LastElidedTimestampResponse::generated_message_descriptor_data());
            messages.push(InternodeMetricsRequest::generated_message_descriptor_data());
            messages.push(InternodeMetricsResponse::generated_message_descriptor_data());
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
