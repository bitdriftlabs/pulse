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

//! Generated file from `pulse/config/inflow/v1/prom_remote_write.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.inflow.v1.PromRemoteWriteServerConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct PromRemoteWriteServerConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.bind)
    pub bind: ::protobuf::Chars,
    // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.parse_config)
    pub parse_config: ::protobuf::MessageField<prom_remote_write_server_config::ParseConfig>,
    // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.downstream_id_source)
    pub downstream_id_source: ::protobuf::MessageField<prom_remote_write_server_config::DownstreamIdSource>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a PromRemoteWriteServerConfig {
    fn default() -> &'a PromRemoteWriteServerConfig {
        <PromRemoteWriteServerConfig as ::protobuf::Message>::default_instance()
    }
}

impl PromRemoteWriteServerConfig {
    pub fn new() -> PromRemoteWriteServerConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(3);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "bind",
            |m: &PromRemoteWriteServerConfig| { &m.bind },
            |m: &mut PromRemoteWriteServerConfig| { &mut m.bind },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, prom_remote_write_server_config::ParseConfig>(
            "parse_config",
            |m: &PromRemoteWriteServerConfig| { &m.parse_config },
            |m: &mut PromRemoteWriteServerConfig| { &mut m.parse_config },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, prom_remote_write_server_config::DownstreamIdSource>(
            "downstream_id_source",
            |m: &PromRemoteWriteServerConfig| { &m.downstream_id_source },
            |m: &mut PromRemoteWriteServerConfig| { &mut m.downstream_id_source },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<PromRemoteWriteServerConfig>(
            "PromRemoteWriteServerConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for PromRemoteWriteServerConfig {
    const NAME: &'static str = "PromRemoteWriteServerConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.bind = is.read_tokio_chars()?;
                },
                18 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.parse_config)?;
                },
                26 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.downstream_id_source)?;
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
        if !self.bind.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.bind);
        }
        if let Some(v) = self.parse_config.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        if let Some(v) = self.downstream_id_source.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if !self.bind.is_empty() {
            os.write_string(1, &self.bind)?;
        }
        if let Some(v) = self.parse_config.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
        }
        if let Some(v) = self.downstream_id_source.as_ref() {
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

    fn new() -> PromRemoteWriteServerConfig {
        PromRemoteWriteServerConfig::new()
    }

    fn clear(&mut self) {
        self.bind.clear();
        self.parse_config.clear();
        self.downstream_id_source.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static PromRemoteWriteServerConfig {
        static instance: PromRemoteWriteServerConfig = PromRemoteWriteServerConfig {
            bind: ::protobuf::Chars::new(),
            parse_config: ::protobuf::MessageField::none(),
            downstream_id_source: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for PromRemoteWriteServerConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("PromRemoteWriteServerConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for PromRemoteWriteServerConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for PromRemoteWriteServerConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

/// Nested message and enums of message `PromRemoteWriteServerConfig`
pub mod prom_remote_write_server_config {
    // @@protoc_insertion_point(message:pulse.config.inflow.v1.PromRemoteWriteServerConfig.ParseConfig)
    #[derive(PartialEq,Clone,Default,Debug)]
    pub struct ParseConfig {
        // message fields
        // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.ParseConfig.summary_as_timer)
        pub summary_as_timer: bool,
        // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.ParseConfig.counter_as_delta)
        pub counter_as_delta: bool,
        // @@protoc_insertion_point(field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.ParseConfig.ignore_duplicate_metadata)
        pub ignore_duplicate_metadata: bool,
        // special fields
        // @@protoc_insertion_point(special_field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.ParseConfig.special_fields)
        pub special_fields: ::protobuf::SpecialFields,
    }

    impl<'a> ::std::default::Default for &'a ParseConfig {
        fn default() -> &'a ParseConfig {
            <ParseConfig as ::protobuf::Message>::default_instance()
        }
    }

    impl ParseConfig {
        pub fn new() -> ParseConfig {
            ::std::default::Default::default()
        }

        pub(in super) fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
            let mut fields = ::std::vec::Vec::with_capacity(3);
            let mut oneofs = ::std::vec::Vec::with_capacity(0);
            fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
                "summary_as_timer",
                |m: &ParseConfig| { &m.summary_as_timer },
                |m: &mut ParseConfig| { &mut m.summary_as_timer },
            ));
            fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
                "counter_as_delta",
                |m: &ParseConfig| { &m.counter_as_delta },
                |m: &mut ParseConfig| { &mut m.counter_as_delta },
            ));
            fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
                "ignore_duplicate_metadata",
                |m: &ParseConfig| { &m.ignore_duplicate_metadata },
                |m: &mut ParseConfig| { &mut m.ignore_duplicate_metadata },
            ));
            ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<ParseConfig>(
                "PromRemoteWriteServerConfig.ParseConfig",
                fields,
                oneofs,
            )
        }
    }

    impl ::protobuf::Message for ParseConfig {
        const NAME: &'static str = "ParseConfig";

        fn is_initialized(&self) -> bool {
            true
        }

        fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
            while let Some(tag) = is.read_raw_tag_or_eof()? {
                match tag {
                    8 => {
                        self.summary_as_timer = is.read_bool()?;
                    },
                    16 => {
                        self.counter_as_delta = is.read_bool()?;
                    },
                    24 => {
                        self.ignore_duplicate_metadata = is.read_bool()?;
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
            if self.summary_as_timer != false {
                my_size += 1 + 1;
            }
            if self.counter_as_delta != false {
                my_size += 1 + 1;
            }
            if self.ignore_duplicate_metadata != false {
                my_size += 1 + 1;
            }
            my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
            self.special_fields.cached_size().set(my_size as u32);
            my_size
        }

        fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
            if self.summary_as_timer != false {
                os.write_bool(1, self.summary_as_timer)?;
            }
            if self.counter_as_delta != false {
                os.write_bool(2, self.counter_as_delta)?;
            }
            if self.ignore_duplicate_metadata != false {
                os.write_bool(3, self.ignore_duplicate_metadata)?;
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

        fn new() -> ParseConfig {
            ParseConfig::new()
        }

        fn clear(&mut self) {
            self.summary_as_timer = false;
            self.counter_as_delta = false;
            self.ignore_duplicate_metadata = false;
            self.special_fields.clear();
        }

        fn default_instance() -> &'static ParseConfig {
            static instance: ParseConfig = ParseConfig {
                summary_as_timer: false,
                counter_as_delta: false,
                ignore_duplicate_metadata: false,
                special_fields: ::protobuf::SpecialFields::new(),
            };
            &instance
        }
    }

    impl ::protobuf::MessageFull for ParseConfig {
        fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| super::file_descriptor().message_by_package_relative_name("PromRemoteWriteServerConfig.ParseConfig").unwrap()).clone()
        }
    }

    impl ::std::fmt::Display for ParseConfig {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            ::protobuf::text_format::fmt(self, f)
        }
    }

    impl ::protobuf::reflect::ProtobufValue for ParseConfig {
        type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
    }

    // @@protoc_insertion_point(message:pulse.config.inflow.v1.PromRemoteWriteServerConfig.DownstreamIdSource)
    #[derive(PartialEq,Clone,Default,Debug)]
    pub struct DownstreamIdSource {
        // message oneof groups
        pub source_type: ::std::option::Option<downstream_id_source::Source_type>,
        // special fields
        // @@protoc_insertion_point(special_field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.DownstreamIdSource.special_fields)
        pub special_fields: ::protobuf::SpecialFields,
    }

    impl<'a> ::std::default::Default for &'a DownstreamIdSource {
        fn default() -> &'a DownstreamIdSource {
            <DownstreamIdSource as ::protobuf::Message>::default_instance()
        }
    }

    impl DownstreamIdSource {
        pub fn new() -> DownstreamIdSource {
            ::std::default::Default::default()
        }

        // bool remote_ip = 1;

        pub fn remote_ip(&self) -> bool {
            match self.source_type {
                ::std::option::Option::Some(downstream_id_source::Source_type::RemoteIp(v)) => v,
                _ => false,
            }
        }

        pub fn clear_remote_ip(&mut self) {
            self.source_type = ::std::option::Option::None;
        }

        pub fn has_remote_ip(&self) -> bool {
            match self.source_type {
                ::std::option::Option::Some(downstream_id_source::Source_type::RemoteIp(..)) => true,
                _ => false,
            }
        }

        // Param is passed by value, moved
        pub fn set_remote_ip(&mut self, v: bool) {
            self.source_type = ::std::option::Option::Some(downstream_id_source::Source_type::RemoteIp(v))
        }

        // string request_header = 2;

        pub fn request_header(&self) -> &str {
            match self.source_type {
                ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(ref v)) => v,
                _ => "",
            }
        }

        pub fn clear_request_header(&mut self) {
            self.source_type = ::std::option::Option::None;
        }

        pub fn has_request_header(&self) -> bool {
            match self.source_type {
                ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(..)) => true,
                _ => false,
            }
        }

        // Param is passed by value, moved
        pub fn set_request_header(&mut self, v: ::protobuf::Chars) {
            self.source_type = ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(v))
        }

        // Mutable pointer to the field.
        pub fn mut_request_header(&mut self) -> &mut ::protobuf::Chars {
            if let ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(_)) = self.source_type {
            } else {
                self.source_type = ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(::protobuf::Chars::new()));
            }
            match self.source_type {
                ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(ref mut v)) => v,
                _ => panic!(),
            }
        }

        // Take field
        pub fn take_request_header(&mut self) -> ::protobuf::Chars {
            if self.has_request_header() {
                match self.source_type.take() {
                    ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(v)) => v,
                    _ => panic!(),
                }
            } else {
                ::protobuf::Chars::new()
            }
        }

        pub(in super) fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
            let mut fields = ::std::vec::Vec::with_capacity(2);
            let mut oneofs = ::std::vec::Vec::with_capacity(1);
            fields.push(::protobuf::reflect::rt::v2::make_oneof_copy_has_get_set_simpler_accessors::<_, _>(
                "remote_ip",
                DownstreamIdSource::has_remote_ip,
                DownstreamIdSource::remote_ip,
                DownstreamIdSource::set_remote_ip,
            ));
            fields.push(::protobuf::reflect::rt::v2::make_oneof_deref_has_get_set_simpler_accessor::<_, _>(
                "request_header",
                DownstreamIdSource::has_request_header,
                DownstreamIdSource::request_header,
                DownstreamIdSource::set_request_header,
            ));
            oneofs.push(downstream_id_source::Source_type::generated_oneof_descriptor_data());
            ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<DownstreamIdSource>(
                "PromRemoteWriteServerConfig.DownstreamIdSource",
                fields,
                oneofs,
            )
        }
    }

    impl ::protobuf::Message for DownstreamIdSource {
        const NAME: &'static str = "DownstreamIdSource";

        fn is_initialized(&self) -> bool {
            true
        }

        fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
            while let Some(tag) = is.read_raw_tag_or_eof()? {
                match tag {
                    8 => {
                        self.source_type = ::std::option::Option::Some(downstream_id_source::Source_type::RemoteIp(is.read_bool()?));
                    },
                    18 => {
                        self.source_type = ::std::option::Option::Some(downstream_id_source::Source_type::RequestHeader(is.read_tokio_chars()?));
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
            if let ::std::option::Option::Some(ref v) = self.source_type {
                match v {
                    &downstream_id_source::Source_type::RemoteIp(v) => {
                        my_size += 1 + 1;
                    },
                    &downstream_id_source::Source_type::RequestHeader(ref v) => {
                        my_size += ::protobuf::rt::string_size(2, &v);
                    },
                };
            }
            my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
            self.special_fields.cached_size().set(my_size as u32);
            my_size
        }

        fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
            if let ::std::option::Option::Some(ref v) = self.source_type {
                match v {
                    &downstream_id_source::Source_type::RemoteIp(v) => {
                        os.write_bool(1, v)?;
                    },
                    &downstream_id_source::Source_type::RequestHeader(ref v) => {
                        os.write_string(2, v)?;
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

        fn new() -> DownstreamIdSource {
            DownstreamIdSource::new()
        }

        fn clear(&mut self) {
            self.source_type = ::std::option::Option::None;
            self.source_type = ::std::option::Option::None;
            self.special_fields.clear();
        }

        fn default_instance() -> &'static DownstreamIdSource {
            static instance: DownstreamIdSource = DownstreamIdSource {
                source_type: ::std::option::Option::None,
                special_fields: ::protobuf::SpecialFields::new(),
            };
            &instance
        }
    }

    impl ::protobuf::MessageFull for DownstreamIdSource {
        fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| super::file_descriptor().message_by_package_relative_name("PromRemoteWriteServerConfig.DownstreamIdSource").unwrap()).clone()
        }
    }

    impl ::std::fmt::Display for DownstreamIdSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            ::protobuf::text_format::fmt(self, f)
        }
    }

    impl ::protobuf::reflect::ProtobufValue for DownstreamIdSource {
        type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
    }

    /// Nested message and enums of message `DownstreamIdSource`
    pub mod downstream_id_source {

        #[derive(Clone,PartialEq,Debug)]
        // @@protoc_insertion_point(oneof:pulse.config.inflow.v1.PromRemoteWriteServerConfig.DownstreamIdSource.source_type)
        pub enum Source_type {
            // @@protoc_insertion_point(oneof_field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.DownstreamIdSource.remote_ip)
            RemoteIp(bool),
            // @@protoc_insertion_point(oneof_field:pulse.config.inflow.v1.PromRemoteWriteServerConfig.DownstreamIdSource.request_header)
            RequestHeader(::protobuf::Chars),
        }

        impl ::protobuf::Oneof for Source_type {
        }

        impl ::protobuf::OneofFull for Source_type {
            fn descriptor() -> ::protobuf::reflect::OneofDescriptor {
                static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::OneofDescriptor> = ::protobuf::rt::Lazy::new();
                descriptor.get(|| <super::DownstreamIdSource as ::protobuf::MessageFull>::descriptor().oneof_by_name("source_type").unwrap()).clone()
            }
        }

        impl Source_type {
            pub(in super::super) fn generated_oneof_descriptor_data() -> ::protobuf::reflect::GeneratedOneofDescriptorData {
                ::protobuf::reflect::GeneratedOneofDescriptorData::new::<Source_type>("source_type")
            }
        }
    }
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n.pulse/config/inflow/v1/prom_remote_write.proto\x12\x16pulse.config.in\
    flow.v1\x1a\x17validate/validate.proto\"\xbd\x04\n\x1bPromRemoteWriteSer\
    verConfig\x12\x1b\n\x04bind\x18\x01\x20\x01(\tR\x04bindB\x07\xfaB\x04r\
    \x02\x10\x01\x12b\n\x0cparse_config\x18\x02\x20\x01(\x0b2?.pulse.config.\
    inflow.v1.PromRemoteWriteServerConfig.ParseConfigR\x0bparseConfig\x12x\n\
    \x14downstream_id_source\x18\x03\x20\x01(\x0b2F.pulse.config.inflow.v1.P\
    romRemoteWriteServerConfig.DownstreamIdSourceR\x12downstreamIdSource\x1a\
    \x9d\x01\n\x0bParseConfig\x12(\n\x10summary_as_timer\x18\x01\x20\x01(\
    \x08R\x0esummaryAsTimer\x12(\n\x10counter_as_delta\x18\x02\x20\x01(\x08R\
    \x0ecounterAsDelta\x12:\n\x19ignore_duplicate_metadata\x18\x03\x20\x01(\
    \x08R\x17ignoreDuplicateMetadata\x1a\x82\x01\n\x12DownstreamIdSource\x12\
    &\n\tremote_ip\x18\x01\x20\x01(\x08H\0R\x08remoteIpB\x07\xfaB\x04j\x02\
    \x08\x01\x120\n\x0erequest_header\x18\x02\x20\x01(\tH\0R\rrequestHeaderB\
    \x07\xfaB\x04r\x02\x10\x01B\x12\n\x0bsource_type\x12\x03\xf8B\x01b\x06pr\
    oto3\
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
            let mut messages = ::std::vec::Vec::with_capacity(3);
            messages.push(PromRemoteWriteServerConfig::generated_message_descriptor_data());
            messages.push(prom_remote_write_server_config::ParseConfig::generated_message_descriptor_data());
            messages.push(prom_remote_write_server_config::DownstreamIdSource::generated_message_descriptor_data());
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