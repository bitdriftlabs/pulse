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

//! Generated file from `pulse/config/processor/v1/internode.proto`

/// Generated files are compatible only with the same version
/// of protobuf runtime.
const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_4_0_0_ALPHA_0;

// @@protoc_insertion_point(message:pulse.config.processor.v1.InternodeConfig)
#[derive(PartialEq,Clone,Default,Debug)]
pub struct InternodeConfig {
    // message fields
    // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.listen)
    pub listen: ::protobuf::Chars,
    // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.total_nodes)
    pub total_nodes: ::std::option::Option<u32>,
    // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.this_node_id)
    pub this_node_id: ::protobuf::Chars,
    // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.nodes)
    pub nodes: ::std::vec::Vec<internode_config::NodeConfig>,
    // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.request_policy)
    pub request_policy: ::protobuf::MessageField<internode_config::RequestPolicy>,
    // special fields
    // @@protoc_insertion_point(special_field:pulse.config.processor.v1.InternodeConfig.special_fields)
    pub special_fields: ::protobuf::SpecialFields,
}

impl<'a> ::std::default::Default for &'a InternodeConfig {
    fn default() -> &'a InternodeConfig {
        <InternodeConfig as ::protobuf::Message>::default_instance()
    }
}

impl InternodeConfig {
    pub fn new() -> InternodeConfig {
        ::std::default::Default::default()
    }

    fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
        let mut fields = ::std::vec::Vec::with_capacity(5);
        let mut oneofs = ::std::vec::Vec::with_capacity(0);
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "listen",
            |m: &InternodeConfig| { &m.listen },
            |m: &mut InternodeConfig| { &mut m.listen },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
            "total_nodes",
            |m: &InternodeConfig| { &m.total_nodes },
            |m: &mut InternodeConfig| { &mut m.total_nodes },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
            "this_node_id",
            |m: &InternodeConfig| { &m.this_node_id },
            |m: &mut InternodeConfig| { &mut m.this_node_id },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_vec_simpler_accessor::<_, _>(
            "nodes",
            |m: &InternodeConfig| { &m.nodes },
            |m: &mut InternodeConfig| { &mut m.nodes },
        ));
        fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, internode_config::RequestPolicy>(
            "request_policy",
            |m: &InternodeConfig| { &m.request_policy },
            |m: &mut InternodeConfig| { &mut m.request_policy },
        ));
        ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<InternodeConfig>(
            "InternodeConfig",
            fields,
            oneofs,
        )
    }
}

impl ::protobuf::Message for InternodeConfig {
    const NAME: &'static str = "InternodeConfig";

    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
        while let Some(tag) = is.read_raw_tag_or_eof()? {
            match tag {
                10 => {
                    self.listen = is.read_tokio_chars()?;
                },
                16 => {
                    self.total_nodes = ::std::option::Option::Some(is.read_uint32()?);
                },
                26 => {
                    self.this_node_id = is.read_tokio_chars()?;
                },
                34 => {
                    self.nodes.push(is.read_message()?);
                },
                42 => {
                    ::protobuf::rt::read_singular_message_into_field(is, &mut self.request_policy)?;
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
        if !self.listen.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.listen);
        }
        if let Some(v) = self.total_nodes {
            my_size += ::protobuf::rt::uint32_size(2, v);
        }
        if !self.this_node_id.is_empty() {
            my_size += ::protobuf::rt::string_size(3, &self.this_node_id);
        }
        for value in &self.nodes {
            let len = value.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        };
        if let Some(v) = self.request_policy.as_ref() {
            let len = v.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
        }
        my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
        self.special_fields.cached_size().set(my_size as u32);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
        if !self.listen.is_empty() {
            os.write_string(1, &self.listen)?;
        }
        if let Some(v) = self.total_nodes {
            os.write_uint32(2, v)?;
        }
        if !self.this_node_id.is_empty() {
            os.write_string(3, &self.this_node_id)?;
        }
        for v in &self.nodes {
            ::protobuf::rt::write_message_field_with_cached_size(4, v, os)?;
        };
        if let Some(v) = self.request_policy.as_ref() {
            ::protobuf::rt::write_message_field_with_cached_size(5, v, os)?;
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

    fn new() -> InternodeConfig {
        InternodeConfig::new()
    }

    fn clear(&mut self) {
        self.listen.clear();
        self.total_nodes = ::std::option::Option::None;
        self.this_node_id.clear();
        self.nodes.clear();
        self.request_policy.clear();
        self.special_fields.clear();
    }

    fn default_instance() -> &'static InternodeConfig {
        static instance: InternodeConfig = InternodeConfig {
            listen: ::protobuf::Chars::new(),
            total_nodes: ::std::option::Option::None,
            this_node_id: ::protobuf::Chars::new(),
            nodes: ::std::vec::Vec::new(),
            request_policy: ::protobuf::MessageField::none(),
            special_fields: ::protobuf::SpecialFields::new(),
        };
        &instance
    }
}

impl ::protobuf::MessageFull for InternodeConfig {
    fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
        static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
        descriptor.get(|| file_descriptor().message_by_package_relative_name("InternodeConfig").unwrap()).clone()
    }
}

impl ::std::fmt::Display for InternodeConfig {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for InternodeConfig {
    type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
}

/// Nested message and enums of message `InternodeConfig`
pub mod internode_config {
    // @@protoc_insertion_point(message:pulse.config.processor.v1.InternodeConfig.NodeConfig)
    #[derive(PartialEq,Clone,Default,Debug)]
    pub struct NodeConfig {
        // message fields
        // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.NodeConfig.node_id)
        pub node_id: ::protobuf::Chars,
        // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.NodeConfig.address)
        pub address: ::protobuf::Chars,
        // special fields
        // @@protoc_insertion_point(special_field:pulse.config.processor.v1.InternodeConfig.NodeConfig.special_fields)
        pub special_fields: ::protobuf::SpecialFields,
    }

    impl<'a> ::std::default::Default for &'a NodeConfig {
        fn default() -> &'a NodeConfig {
            <NodeConfig as ::protobuf::Message>::default_instance()
        }
    }

    impl NodeConfig {
        pub fn new() -> NodeConfig {
            ::std::default::Default::default()
        }

        pub(in super) fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
            let mut fields = ::std::vec::Vec::with_capacity(2);
            let mut oneofs = ::std::vec::Vec::with_capacity(0);
            fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
                "node_id",
                |m: &NodeConfig| { &m.node_id },
                |m: &mut NodeConfig| { &mut m.node_id },
            ));
            fields.push(::protobuf::reflect::rt::v2::make_simpler_field_accessor::<_, _>(
                "address",
                |m: &NodeConfig| { &m.address },
                |m: &mut NodeConfig| { &mut m.address },
            ));
            ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<NodeConfig>(
                "InternodeConfig.NodeConfig",
                fields,
                oneofs,
            )
        }
    }

    impl ::protobuf::Message for NodeConfig {
        const NAME: &'static str = "NodeConfig";

        fn is_initialized(&self) -> bool {
            true
        }

        fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
            while let Some(tag) = is.read_raw_tag_or_eof()? {
                match tag {
                    10 => {
                        self.node_id = is.read_tokio_chars()?;
                    },
                    18 => {
                        self.address = is.read_tokio_chars()?;
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
            if !self.node_id.is_empty() {
                my_size += ::protobuf::rt::string_size(1, &self.node_id);
            }
            if !self.address.is_empty() {
                my_size += ::protobuf::rt::string_size(2, &self.address);
            }
            my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
            self.special_fields.cached_size().set(my_size as u32);
            my_size
        }

        fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
            if !self.node_id.is_empty() {
                os.write_string(1, &self.node_id)?;
            }
            if !self.address.is_empty() {
                os.write_string(2, &self.address)?;
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

        fn new() -> NodeConfig {
            NodeConfig::new()
        }

        fn clear(&mut self) {
            self.node_id.clear();
            self.address.clear();
            self.special_fields.clear();
        }

        fn default_instance() -> &'static NodeConfig {
            static instance: NodeConfig = NodeConfig {
                node_id: ::protobuf::Chars::new(),
                address: ::protobuf::Chars::new(),
                special_fields: ::protobuf::SpecialFields::new(),
            };
            &instance
        }
    }

    impl ::protobuf::MessageFull for NodeConfig {
        fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| super::file_descriptor().message_by_package_relative_name("InternodeConfig.NodeConfig").unwrap()).clone()
        }
    }

    impl ::std::fmt::Display for NodeConfig {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            ::protobuf::text_format::fmt(self, f)
        }
    }

    impl ::protobuf::reflect::ProtobufValue for NodeConfig {
        type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
    }

    // @@protoc_insertion_point(message:pulse.config.processor.v1.InternodeConfig.RequestPolicy)
    #[derive(PartialEq,Clone,Default,Debug)]
    pub struct RequestPolicy {
        // message fields
        // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.RequestPolicy.timeout)
        pub timeout: ::protobuf::MessageField<::protobuf::well_known_types::duration::Duration>,
        // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.RequestPolicy.retry_policy)
        pub retry_policy: ::protobuf::MessageField<super::super::retry::RetryPolicy>,
        // @@protoc_insertion_point(field:pulse.config.processor.v1.InternodeConfig.RequestPolicy.max_concurrent_requests)
        pub max_concurrent_requests: ::std::option::Option<u32>,
        // special fields
        // @@protoc_insertion_point(special_field:pulse.config.processor.v1.InternodeConfig.RequestPolicy.special_fields)
        pub special_fields: ::protobuf::SpecialFields,
    }

    impl<'a> ::std::default::Default for &'a RequestPolicy {
        fn default() -> &'a RequestPolicy {
            <RequestPolicy as ::protobuf::Message>::default_instance()
        }
    }

    impl RequestPolicy {
        pub fn new() -> RequestPolicy {
            ::std::default::Default::default()
        }

        pub(in super) fn generated_message_descriptor_data() -> ::protobuf::reflect::GeneratedMessageDescriptorData {
            let mut fields = ::std::vec::Vec::with_capacity(3);
            let mut oneofs = ::std::vec::Vec::with_capacity(0);
            fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, ::protobuf::well_known_types::duration::Duration>(
                "timeout",
                |m: &RequestPolicy| { &m.timeout },
                |m: &mut RequestPolicy| { &mut m.timeout },
            ));
            fields.push(::protobuf::reflect::rt::v2::make_message_field_accessor::<_, super::super::retry::RetryPolicy>(
                "retry_policy",
                |m: &RequestPolicy| { &m.retry_policy },
                |m: &mut RequestPolicy| { &mut m.retry_policy },
            ));
            fields.push(::protobuf::reflect::rt::v2::make_option_accessor::<_, _>(
                "max_concurrent_requests",
                |m: &RequestPolicy| { &m.max_concurrent_requests },
                |m: &mut RequestPolicy| { &mut m.max_concurrent_requests },
            ));
            ::protobuf::reflect::GeneratedMessageDescriptorData::new_2::<RequestPolicy>(
                "InternodeConfig.RequestPolicy",
                fields,
                oneofs,
            )
        }
    }

    impl ::protobuf::Message for RequestPolicy {
        const NAME: &'static str = "RequestPolicy";

        fn is_initialized(&self) -> bool {
            true
        }

        fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::Result<()> {
            while let Some(tag) = is.read_raw_tag_or_eof()? {
                match tag {
                    10 => {
                        ::protobuf::rt::read_singular_message_into_field(is, &mut self.timeout)?;
                    },
                    18 => {
                        ::protobuf::rt::read_singular_message_into_field(is, &mut self.retry_policy)?;
                    },
                    24 => {
                        self.max_concurrent_requests = ::std::option::Option::Some(is.read_uint32()?);
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
            if let Some(v) = self.timeout.as_ref() {
                let len = v.compute_size();
                my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
            }
            if let Some(v) = self.retry_policy.as_ref() {
                let len = v.compute_size();
                my_size += 1 + ::protobuf::rt::compute_raw_varint64_size(len) + len;
            }
            if let Some(v) = self.max_concurrent_requests {
                my_size += ::protobuf::rt::uint32_size(3, v);
            }
            my_size += ::protobuf::rt::unknown_fields_size(self.special_fields.unknown_fields());
            self.special_fields.cached_size().set(my_size as u32);
            my_size
        }

        fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::Result<()> {
            if let Some(v) = self.timeout.as_ref() {
                ::protobuf::rt::write_message_field_with_cached_size(1, v, os)?;
            }
            if let Some(v) = self.retry_policy.as_ref() {
                ::protobuf::rt::write_message_field_with_cached_size(2, v, os)?;
            }
            if let Some(v) = self.max_concurrent_requests {
                os.write_uint32(3, v)?;
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

        fn new() -> RequestPolicy {
            RequestPolicy::new()
        }

        fn clear(&mut self) {
            self.timeout.clear();
            self.retry_policy.clear();
            self.max_concurrent_requests = ::std::option::Option::None;
            self.special_fields.clear();
        }

        fn default_instance() -> &'static RequestPolicy {
            static instance: RequestPolicy = RequestPolicy {
                timeout: ::protobuf::MessageField::none(),
                retry_policy: ::protobuf::MessageField::none(),
                max_concurrent_requests: ::std::option::Option::None,
                special_fields: ::protobuf::SpecialFields::new(),
            };
            &instance
        }
    }

    impl ::protobuf::MessageFull for RequestPolicy {
        fn descriptor() -> ::protobuf::reflect::MessageDescriptor {
            static descriptor: ::protobuf::rt::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::rt::Lazy::new();
            descriptor.get(|| super::file_descriptor().message_by_package_relative_name("InternodeConfig.RequestPolicy").unwrap()).clone()
        }
    }

    impl ::std::fmt::Display for RequestPolicy {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            ::protobuf::text_format::fmt(self, f)
        }
    }

    impl ::protobuf::reflect::ProtobufValue for RequestPolicy {
        type RuntimeType = ::protobuf::reflect::rt::RuntimeTypeMessage<Self>;
    }
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n)pulse/config/processor/v1/internode.proto\x12\x19pulse.config.process\
    or.v1\x1a\"pulse/config/common/v1/retry.proto\x1a\x1egoogle/protobuf/dur\
    ation.proto\x1a\x17validate/validate.proto\"\x9a\x05\n\x0fInternodeConfi\
    g\x12\x1f\n\x06listen\x18\x01\x20\x01(\tR\x06listenB\x07\xfaB\x04r\x02\
    \x10\x01\x12$\n\x0btotal_nodes\x18\x02\x20\x01(\rH\0R\ntotalNodes\x88\
    \x01\x01\x12)\n\x0cthis_node_id\x18\x03\x20\x01(\tR\nthisNodeIdB\x07\xfa\
    B\x04r\x02\x10\x01\x12U\n\x05nodes\x18\x04\x20\x03(\x0b25.pulse.config.p\
    rocessor.v1.InternodeConfig.NodeConfigR\x05nodesB\x08\xfaB\x05\x92\x01\
    \x02\x08\x01\x12i\n\x0erequest_policy\x18\x05\x20\x01(\x0b28.pulse.confi\
    g.processor.v1.InternodeConfig.RequestPolicyR\rrequestPolicyB\x08\xfaB\
    \x05\xaa\x01\x02*\0\x1aQ\n\nNodeConfig\x12\x20\n\x07node_id\x18\x01\x20\
    \x01(\tR\x06nodeIdB\x07\xfaB\x04r\x02\x10\x01\x12!\n\x07address\x18\x02\
    \x20\x01(\tR\x07addressB\x07\xfaB\x04r\x02\x10\x01\x1a\xef\x01\n\rReques\
    tPolicy\x12=\n\x07timeout\x18\x01\x20\x01(\x0b2\x19.google.protobuf.Dura\
    tionR\x07timeoutB\x08\xfaB\x05\xaa\x01\x02*\0\x12F\n\x0cretry_policy\x18\
    \x02\x20\x01(\x0b2#.pulse.config.common.v1.RetryPolicyR\x0bretryPolicy\
    \x12;\n\x17max_concurrent_requests\x18\x03\x20\x01(\rH\0R\x15maxConcurre\
    ntRequests\x88\x01\x01B\x1a\n\x18_max_concurrent_requestsB\x0e\n\x0c_tot\
    al_nodesb\x06proto3\
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
            deps.push(super::retry::file_descriptor().clone());
            deps.push(::protobuf::well_known_types::duration::file_descriptor().clone());
            deps.push(super::validate::file_descriptor().clone());
            let mut messages = ::std::vec::Vec::with_capacity(3);
            messages.push(InternodeConfig::generated_message_descriptor_data());
            messages.push(internode_config::NodeConfig::generated_message_descriptor_data());
            messages.push(internode_config::RequestPolicy::generated_message_descriptor_data());
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
