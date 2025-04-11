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

//! Generated file from `pulse/otlp/v1/metrics_service.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.otlp.collector.metrics.v1.ExportMetricsServiceRequest)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct ExportMetricsServiceRequest {
    // message fields
    // @@protoc_insertion_point(field:pulse.otlp.collector.metrics.v1.ExportMetricsServiceRequest.resource_metrics)
    pub resource_metrics: ::std::vec::Vec<super::metrics::ResourceMetrics>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.otlp.collector.metrics.v1.ExportMetricsServiceRequest.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a ExportMetricsServiceRequest {
    fn default() -> &'a ExportMetricsServiceRequest {
        <ExportMetricsServiceRequest as ::protobuf::Message>::default_instance()
    }
}

impl ExportMetricsServiceRequest {
    pub fn new() -> ExportMetricsServiceRequest {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_vec_simpler_accessor::<_, _>(
            "resource_metrics",
            |m: &ExportMetricsServiceRequest| { &m.resource_metrics },
            |m: &mut ExportMetricsServiceRequest| { &mut m.resource_metrics },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<ExportMetricsServiceRequest>(
            "ExportMetricsServiceRequest",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for ExportMetricsServiceRequest {
    const NAME: &'static str = "ExportMetricsServiceRequest";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.resource_metrics.push(is.read_message()?);
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
        for value in &self.resource_metrics {
            let len = value.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        for v in &self.resource_metrics {
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

    fn new() -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest::new()
    }

    fn clear(&mut self) {
        self.resource_metrics.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static ExportMetricsServiceRequest {
        static instance: ExportMetricsServiceRequest = ExportMetricsServiceRequest {
            resource_metrics: ::std::vec::Vec::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for ExportMetricsServiceRequest {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("ExportMetricsServiceRequest").unwrap()).clone()
    }
}

impl ::std::fmt::Display for ExportMetricsServiceRequest {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for ExportMetricsServiceRequest {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.otlp.collector.metrics.v1.ExportMetricsServiceResponse)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct ExportMetricsServiceResponse {
    // message fields
    // @@protoc_insertion_point(field:pulse.otlp.collector.metrics.v1.ExportMetricsServiceResponse.partial_success)
    pub partial_success: ::protobuf::MessageField<ExportMetricsPartialSuccess>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.otlp.collector.metrics.v1.ExportMetricsServiceResponse.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a ExportMetricsServiceResponse {
    fn default() -> &'a ExportMetricsServiceResponse {
        <ExportMetricsServiceResponse as ::protobuf::Message>::default_instance()
    }
}

impl ExportMetricsServiceResponse {
    pub fn new() -> ExportMetricsServiceResponse {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(1);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ExportMetricsPartialSuccess>(
            "partial_success",
            |m: &ExportMetricsServiceResponse| { &m.partial_success },
            |m: &mut ExportMetricsServiceResponse| { &mut m.partial_success },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<ExportMetricsServiceResponse>(
            "ExportMetricsServiceResponse",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for ExportMetricsServiceResponse {
    const NAME: &'static str = "ExportMetricsServiceResponse";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.partial_success)?;
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
        if let Some(v) = self.partial_success.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if let Some(v) = self.partial_success.as_ref() {
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

    fn new() -> ExportMetricsServiceResponse {
        ExportMetricsServiceResponse::new()
    }

    fn clear(&mut self) {
        self.partial_success.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static ExportMetricsServiceResponse {
        static instance: ExportMetricsServiceResponse = ExportMetricsServiceResponse {
            partial_success: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for ExportMetricsServiceResponse {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("ExportMetricsServiceResponse").unwrap()).clone()
    }
}

impl ::std::fmt::Display for ExportMetricsServiceResponse {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for ExportMetricsServiceResponse {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

// @@protoc_insertion_point(message:pulse.otlp.collector.metrics.v1.ExportMetricsPartialSuccess)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct ExportMetricsPartialSuccess {
    // message fields
    // @@protoc_insertion_point(field:pulse.otlp.collector.metrics.v1.ExportMetricsPartialSuccess.rejected_data_points)
    pub rejected_data_points: i64,
    // @@protoc_insertion_point(field:pulse.otlp.collector.metrics.v1.ExportMetricsPartialSuccess.error_message)
    pub error_message: ::protobuf::Chars,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.otlp.collector.metrics.v1.ExportMetricsPartialSuccess.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a ExportMetricsPartialSuccess {
    fn default() -> &'a ExportMetricsPartialSuccess {
        <ExportMetricsPartialSuccess as ::protobuf::Message>::default_instance()
    }
}

impl ExportMetricsPartialSuccess {
    pub fn new() -> ExportMetricsPartialSuccess {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(2);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "rejected_data_points",
            |m: &ExportMetricsPartialSuccess| { &m.rejected_data_points },
            |m: &mut ExportMetricsPartialSuccess| { &mut m.rejected_data_points },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "error_message",
            |m: &ExportMetricsPartialSuccess| { &m.error_message },
            |m: &mut ExportMetricsPartialSuccess| { &mut m.error_message },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<ExportMetricsPartialSuccess>(
            "ExportMetricsPartialSuccess",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for ExportMetricsPartialSuccess {
    const NAME: &'static str = "ExportMetricsPartialSuccess";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                8 => {
                    self.rejected_data_points = is.read_int64()?;
                },
                18 => {
                    self.error_message = is.read_tokio_chars()?;
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
        if self.rejected_data_points != 0 {
            my_size += ::protobuf::rt::int64_size(1, self.rejected_data_points);
        }
        if !self.error_message.is_empty() {
            my_size += ::protobuf::rt::string_size(2, &self.error_message);
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if self.rejected_data_points != 0 {
            os.write_int64(1, self.rejected_data_points)?;
        }
        if !self.error_message.is_empty() {
            os.write_string(2, &self.error_message)?;
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

    fn new() -> ExportMetricsPartialSuccess {
        ExportMetricsPartialSuccess::new()
    }

    fn clear(&mut self) {
        self.rejected_data_points = 0;
        self.error_message.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static ExportMetricsPartialSuccess {
        static instance: ExportMetricsPartialSuccess = ExportMetricsPartialSuccess {
            rejected_data_points: 0,
            error_message: ::protobuf::Chars::new(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for ExportMetricsPartialSuccess {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("ExportMetricsPartialSuccess").unwrap()).clone()
    }
}

impl ::std::fmt::Display for ExportMetricsPartialSuccess {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for ExportMetricsPartialSuccess {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n#pulse/otlp/v1/metrics_service.proto\x12\x1fpulse.otlp.collector.metri\
    cs.v1\x1a\x1bpulse/otlp/v1/metrics.proto\"p\n\x1bExportMetricsServiceReq\
    uest\x12Q\n\x10resource_metrics\x18\x01\x20\x03(\x0b2&.pulse.otlp.metric\
    s.v1.ResourceMetricsR\x0fresourceMetrics\"\x85\x01\n\x1cExportMetricsSer\
    viceResponse\x12e\n\x0fpartial_success\x18\x01\x20\x01(\x0b2<.pulse.otlp\
    .collector.metrics.v1.ExportMetricsPartialSuccessR\x0epartialSuccess\"t\
    \n\x1bExportMetricsPartialSuccess\x120\n\x14rejected_data_points\x18\x01\
    \x20\x01(\x03R\x12rejectedDataPoints\x12#\n\rerror_message\x18\x02\x20\
    \x01(\tR\x0cerrorMessage2\x9a\x01\n\x0eMetricsService\x12\x87\x01\n\x06E\
    xport\x12<.pulse.otlp.collector.metrics.v1.ExportMetricsServiceRequest\
    \x1a=.pulse.otlp.collector.metrics.v1.ExportMetricsServiceResponse\"\0b\
    \x06proto3\
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
            deps.push(super::metrics::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(3);
            messages.push(ExportMetricsServiceRequest::generated_message_descriptor_data());
            messages.push(ExportMetricsServiceResponse::generated_message_descriptor_data());
            messages.push(ExportMetricsPartialSuccess::generated_message_descriptor_data());
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
