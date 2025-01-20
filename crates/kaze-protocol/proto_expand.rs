mod proto {
    pub struct Hdr {
        /// basic info
        #[prost(string, tag = "1")]
        pub body_type: ::prost::alloc::string::String,
        #[prost(uint32, tag = "2")]
        pub src_ident: u32,
        /// rpc related info
        #[prost(uint32, tag = "3")]
        pub ret_code: u32,
        #[prost(uint32, tag = "4")]
        pub version: u32,
        /// when has timeout, a response packet is expected
        ///
        /// == 0 means no timeout
        #[prost(uint32, tag = "5")]
        pub timeout: u32,
        /// custom extra data
        #[prost(map = "string, bytes", tag = "9")]
        pub headers: ::std::collections::HashMap<
            ::prost::alloc::string::String,
            ::prost::alloc::vec::Vec<u8>,
        >,
        #[prost(oneof = "hdr::RpcType", tags = "6, 7, 8")]
        pub rpc_type: ::core::option::Option<hdr::RpcType>,
        /// route type
        #[prost(oneof = "hdr::RouteType", tags = "10, 11, 12, 13")]
        pub route_type: ::core::option::Option<hdr::RouteType>,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Hdr {
        #[inline]
        fn clone(&self) -> Hdr {
            Hdr {
                body_type: ::core::clone::Clone::clone(&self.body_type),
                src_ident: ::core::clone::Clone::clone(&self.src_ident),
                ret_code: ::core::clone::Clone::clone(&self.ret_code),
                version: ::core::clone::Clone::clone(&self.version),
                timeout: ::core::clone::Clone::clone(&self.timeout),
                headers: ::core::clone::Clone::clone(&self.headers),
                rpc_type: ::core::clone::Clone::clone(&self.rpc_type),
                route_type: ::core::clone::Clone::clone(&self.route_type),
            }
        }
    }
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for Hdr {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for Hdr {
        #[inline]
        fn eq(&self, other: &Hdr) -> bool {
            self.body_type == other.body_type && self.src_ident == other.src_ident
                && self.ret_code == other.ret_code && self.version == other.version
                && self.timeout == other.timeout && self.headers == other.headers
                && self.rpc_type == other.rpc_type && self.route_type == other.route_type
        }
    }
    impl ::prost::Message for Hdr {
        #[allow(unused_variables)]
        fn encode_raw(&self, buf: &mut impl ::prost::bytes::BufMut) {
            if self.body_type != "" {
                ::prost::encoding::string::encode(1u32, &self.body_type, buf);
            }
            if self.src_ident != 0u32 {
                ::prost::encoding::uint32::encode(2u32, &self.src_ident, buf);
            }
            if self.ret_code != 0u32 {
                ::prost::encoding::uint32::encode(3u32, &self.ret_code, buf);
            }
            if self.version != 0u32 {
                ::prost::encoding::uint32::encode(4u32, &self.version, buf);
            }
            if self.timeout != 0u32 {
                ::prost::encoding::uint32::encode(5u32, &self.timeout, buf);
            }
            if let Some(ref oneof) = self.rpc_type {
                oneof.encode(buf)
            }
            ::prost::encoding::hash_map::encode(
                ::prost::encoding::string::encode,
                ::prost::encoding::string::encoded_len,
                ::prost::encoding::bytes::encode,
                ::prost::encoding::bytes::encoded_len,
                9u32,
                &self.headers,
                buf,
            );
            if let Some(ref oneof) = self.route_type {
                oneof.encode(buf)
            }
        }
        #[allow(unused_variables)]
        fn merge_field(
            &mut self,
            tag: u32,
            wire_type: ::prost::encoding::wire_type::WireType,
            buf: &mut impl ::prost::bytes::Buf,
            ctx: ::prost::encoding::DecodeContext,
        ) -> ::core::result::Result<(), ::prost::DecodeError> {
            const STRUCT_NAME: &'static str = "Hdr";
            match tag {
                1u32 => {
                    let mut value = &mut self.body_type;
                    ::prost::encoding::string::merge(wire_type, value, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "body_type");
                            error
                        })
                }
                2u32 => {
                    let mut value = &mut self.src_ident;
                    ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "src_ident");
                            error
                        })
                }
                3u32 => {
                    let mut value = &mut self.ret_code;
                    ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "ret_code");
                            error
                        })
                }
                4u32 => {
                    let mut value = &mut self.version;
                    ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "version");
                            error
                        })
                }
                5u32 => {
                    let mut value = &mut self.timeout;
                    ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "timeout");
                            error
                        })
                }
                6u32 | 7u32 | 8u32 => {
                    let mut value = &mut self.rpc_type;
                    hdr::RpcType::merge(value, tag, wire_type, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "rpc_type");
                            error
                        })
                }
                9u32 => {
                    let mut value = &mut self.headers;
                    ::prost::encoding::hash_map::merge(
                            ::prost::encoding::string::merge,
                            ::prost::encoding::bytes::merge,
                            &mut value,
                            buf,
                            ctx,
                        )
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "headers");
                            error
                        })
                }
                10u32 | 11u32 | 12u32 | 13u32 => {
                    let mut value = &mut self.route_type;
                    hdr::RouteType::merge(value, tag, wire_type, buf, ctx)
                        .map_err(|mut error| {
                            error.push(STRUCT_NAME, "route_type");
                            error
                        })
                }
                _ => ::prost::encoding::skip_field(wire_type, tag, buf, ctx),
            }
        }
        #[inline]
        fn encoded_len(&self) -> usize {
            0
                + if self.body_type != "" {
                    ::prost::encoding::string::encoded_len(1u32, &self.body_type)
                } else {
                    0
                }
                + if self.src_ident != 0u32 {
                    ::prost::encoding::uint32::encoded_len(2u32, &self.src_ident)
                } else {
                    0
                }
                + if self.ret_code != 0u32 {
                    ::prost::encoding::uint32::encoded_len(3u32, &self.ret_code)
                } else {
                    0
                }
                + if self.version != 0u32 {
                    ::prost::encoding::uint32::encoded_len(4u32, &self.version)
                } else {
                    0
                }
                + if self.timeout != 0u32 {
                    ::prost::encoding::uint32::encoded_len(5u32, &self.timeout)
                } else {
                    0
                } + self.rpc_type.as_ref().map_or(0, hdr::RpcType::encoded_len)
                + ::prost::encoding::hash_map::encoded_len(
                    ::prost::encoding::string::encoded_len,
                    ::prost::encoding::bytes::encoded_len,
                    9u32,
                    &self.headers,
                ) + self.route_type.as_ref().map_or(0, hdr::RouteType::encoded_len)
        }
        fn clear(&mut self) {
            self.body_type.clear();
            self.src_ident = 0u32;
            self.ret_code = 0u32;
            self.version = 0u32;
            self.timeout = 0u32;
            self.rpc_type = ::core::option::Option::None;
            self.headers.clear();
            self.route_type = ::core::option::Option::None;
        }
    }
    impl ::core::default::Default for Hdr {
        fn default() -> Self {
            Hdr {
                body_type: ::prost::alloc::string::String::new(),
                src_ident: 0u32,
                ret_code: 0u32,
                version: 0u32,
                timeout: 0u32,
                rpc_type: ::core::default::Default::default(),
                headers: ::core::default::Default::default(),
                route_type: ::core::default::Default::default(),
            }
        }
    }
    impl ::core::fmt::Debug for Hdr {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let mut builder = f.debug_struct("Hdr");
            let builder = {
                let wrapper = {
                    #[allow(non_snake_case)]
                    fn ScalarWrapper<T>(v: T) -> T {
                        v
                    }
                    ScalarWrapper(&self.body_type)
                };
                builder.field("body_type", &wrapper)
            };
            let builder = {
                let wrapper = {
                    #[allow(non_snake_case)]
                    fn ScalarWrapper<T>(v: T) -> T {
                        v
                    }
                    ScalarWrapper(&self.src_ident)
                };
                builder.field("src_ident", &wrapper)
            };
            let builder = {
                let wrapper = {
                    #[allow(non_snake_case)]
                    fn ScalarWrapper<T>(v: T) -> T {
                        v
                    }
                    ScalarWrapper(&self.ret_code)
                };
                builder.field("ret_code", &wrapper)
            };
            let builder = {
                let wrapper = {
                    #[allow(non_snake_case)]
                    fn ScalarWrapper<T>(v: T) -> T {
                        v
                    }
                    ScalarWrapper(&self.version)
                };
                builder.field("version", &wrapper)
            };
            let builder = {
                let wrapper = {
                    #[allow(non_snake_case)]
                    fn ScalarWrapper<T>(v: T) -> T {
                        v
                    }
                    ScalarWrapper(&self.timeout)
                };
                builder.field("timeout", &wrapper)
            };
            let builder = {
                let wrapper = {
                    struct MapWrapper<'a>(&'a dyn ::core::fmt::Debug);
                    impl<'a> ::core::fmt::Debug for MapWrapper<'a> {
                        fn fmt(
                            &self,
                            f: &mut ::core::fmt::Formatter,
                        ) -> ::core::fmt::Result {
                            self.0.fmt(f)
                        }
                    }
                    MapWrapper(&self.headers)
                };
                builder.field("headers", &wrapper)
            };
            let builder = {
                let wrapper = &self.rpc_type;
                builder.field("rpc_type", &wrapper)
            };
            let builder = {
                let wrapper = &self.route_type;
                builder.field("route_type", &wrapper)
            };
            builder.finish()
        }
    }
    /// Nested message and enum types in `Hdr`.
    pub mod hdr {
        /// ident selector with mask. only match if (dstIdent & mask) == ident
        pub struct DstMask {
            #[prost(uint32, tag = "1")]
            pub ident: u32,
            #[prost(uint32, tag = "2")]
            pub mask: u32,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for DstMask {
            #[inline]
            fn clone(&self) -> DstMask {
                let _: ::core::clone::AssertParamIsClone<u32>;
                *self
            }
        }
        #[automatically_derived]
        impl ::core::marker::Copy for DstMask {}
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for DstMask {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for DstMask {
            #[inline]
            fn eq(&self, other: &DstMask) -> bool {
                self.ident == other.ident && self.mask == other.mask
            }
        }
        impl ::prost::Message for DstMask {
            #[allow(unused_variables)]
            fn encode_raw(&self, buf: &mut impl ::prost::bytes::BufMut) {
                if self.ident != 0u32 {
                    ::prost::encoding::uint32::encode(1u32, &self.ident, buf);
                }
                if self.mask != 0u32 {
                    ::prost::encoding::uint32::encode(2u32, &self.mask, buf);
                }
            }
            #[allow(unused_variables)]
            fn merge_field(
                &mut self,
                tag: u32,
                wire_type: ::prost::encoding::wire_type::WireType,
                buf: &mut impl ::prost::bytes::Buf,
                ctx: ::prost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::prost::DecodeError> {
                const STRUCT_NAME: &'static str = "DstMask";
                match tag {
                    1u32 => {
                        let mut value = &mut self.ident;
                        ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            .map_err(|mut error| {
                                error.push(STRUCT_NAME, "ident");
                                error
                            })
                    }
                    2u32 => {
                        let mut value = &mut self.mask;
                        ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            .map_err(|mut error| {
                                error.push(STRUCT_NAME, "mask");
                                error
                            })
                    }
                    _ => ::prost::encoding::skip_field(wire_type, tag, buf, ctx),
                }
            }
            #[inline]
            fn encoded_len(&self) -> usize {
                0
                    + if self.ident != 0u32 {
                        ::prost::encoding::uint32::encoded_len(1u32, &self.ident)
                    } else {
                        0
                    }
                    + if self.mask != 0u32 {
                        ::prost::encoding::uint32::encoded_len(2u32, &self.mask)
                    } else {
                        0
                    }
            }
            fn clear(&mut self) {
                self.ident = 0u32;
                self.mask = 0u32;
            }
        }
        impl ::core::default::Default for DstMask {
            fn default() -> Self {
                DstMask { ident: 0u32, mask: 0u32 }
            }
        }
        impl ::core::fmt::Debug for DstMask {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let mut builder = f.debug_struct("DstMask");
                let builder = {
                    let wrapper = {
                        #[allow(non_snake_case)]
                        fn ScalarWrapper<T>(v: T) -> T {
                            v
                        }
                        ScalarWrapper(&self.ident)
                    };
                    builder.field("ident", &wrapper)
                };
                let builder = {
                    let wrapper = {
                        #[allow(non_snake_case)]
                        fn ScalarWrapper<T>(v: T) -> T {
                            v
                        }
                        ScalarWrapper(&self.mask)
                    };
                    builder.field("mask", &wrapper)
                };
                builder.finish()
            }
        }
        /// destination ident list for multicast
        pub struct DstMulticast {
            /// all idents that the pkg will be sent to
            #[prost(uint32, repeated, tag = "1")]
            pub dst_idents: ::prost::alloc::vec::Vec<u32>,
        }
        #[automatically_derived]
        impl ::core::clone::Clone for DstMulticast {
            #[inline]
            fn clone(&self) -> DstMulticast {
                DstMulticast {
                    dst_idents: ::core::clone::Clone::clone(&self.dst_idents),
                }
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for DstMulticast {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for DstMulticast {
            #[inline]
            fn eq(&self, other: &DstMulticast) -> bool {
                self.dst_idents == other.dst_idents
            }
        }
        impl ::prost::Message for DstMulticast {
            #[allow(unused_variables)]
            fn encode_raw(&self, buf: &mut impl ::prost::bytes::BufMut) {
                ::prost::encoding::uint32::encode_packed(1u32, &self.dst_idents, buf);
            }
            #[allow(unused_variables)]
            fn merge_field(
                &mut self,
                tag: u32,
                wire_type: ::prost::encoding::wire_type::WireType,
                buf: &mut impl ::prost::bytes::Buf,
                ctx: ::prost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::prost::DecodeError> {
                const STRUCT_NAME: &'static str = "DstMulticast";
                match tag {
                    1u32 => {
                        let mut value = &mut self.dst_idents;
                        ::prost::encoding::uint32::merge_repeated(
                                wire_type,
                                value,
                                buf,
                                ctx,
                            )
                            .map_err(|mut error| {
                                error.push(STRUCT_NAME, "dst_idents");
                                error
                            })
                    }
                    _ => ::prost::encoding::skip_field(wire_type, tag, buf, ctx),
                }
            }
            #[inline]
            fn encoded_len(&self) -> usize {
                0 + ::prost::encoding::uint32::encoded_len_packed(1u32, &self.dst_idents)
            }
            fn clear(&mut self) {
                self.dst_idents.clear();
            }
        }
        impl ::core::default::Default for DstMulticast {
            fn default() -> Self {
                DstMulticast {
                    dst_idents: ::prost::alloc::vec::Vec::new(),
                }
            }
        }
        impl ::core::fmt::Debug for DstMulticast {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let mut builder = f.debug_struct("DstMulticast");
                let builder = {
                    let wrapper = {
                        struct ScalarWrapper<'a>(&'a ::prost::alloc::vec::Vec<u32>);
                        impl<'a> ::core::fmt::Debug for ScalarWrapper<'a> {
                            fn fmt(
                                &self,
                                f: &mut ::core::fmt::Formatter,
                            ) -> ::core::fmt::Result {
                                let mut vec_builder = f.debug_list();
                                for v in self.0 {
                                    #[allow(non_snake_case)]
                                    fn Inner<T>(v: T) -> T {
                                        v
                                    }
                                    vec_builder.entry(&Inner(v));
                                }
                                vec_builder.finish()
                            }
                        }
                        ScalarWrapper(&self.dst_idents)
                    };
                    builder.field("dst_idents", &wrapper)
                };
                builder.finish()
            }
        }
        pub enum RpcType {
            /// request sequence number
            #[prost(uint32, tag = "6")]
            Req(u32),
            /// the request sequence number that responds to
            #[prost(uint32, tag = "7")]
            Rsp(u32),
            /// notify sequence number
            #[prost(uint32, tag = "8")]
            Ntf(u32),
        }
        #[automatically_derived]
        impl ::core::clone::Clone for RpcType {
            #[inline]
            fn clone(&self) -> RpcType {
                let _: ::core::clone::AssertParamIsClone<u32>;
                *self
            }
        }
        #[automatically_derived]
        impl ::core::marker::Copy for RpcType {}
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for RpcType {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for RpcType {
            #[inline]
            fn eq(&self, other: &RpcType) -> bool {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                __self_discr == __arg1_discr
                    && match (self, other) {
                        (RpcType::Req(__self_0), RpcType::Req(__arg1_0)) => {
                            __self_0 == __arg1_0
                        }
                        (RpcType::Rsp(__self_0), RpcType::Rsp(__arg1_0)) => {
                            __self_0 == __arg1_0
                        }
                        (RpcType::Ntf(__self_0), RpcType::Ntf(__arg1_0)) => {
                            __self_0 == __arg1_0
                        }
                        _ => unsafe { ::core::intrinsics::unreachable() }
                    }
            }
        }
        impl RpcType {
            /// Encodes the message to a buffer.
            pub fn encode(&self, buf: &mut impl ::prost::bytes::BufMut) {
                match *self {
                    RpcType::Req(ref value) => {
                        ::prost::encoding::uint32::encode(6u32, &*value, buf);
                    }
                    RpcType::Rsp(ref value) => {
                        ::prost::encoding::uint32::encode(7u32, &*value, buf);
                    }
                    RpcType::Ntf(ref value) => {
                        ::prost::encoding::uint32::encode(8u32, &*value, buf);
                    }
                }
            }
            /// Decodes an instance of the message from a buffer, and merges it into self.
            pub fn merge(
                field: &mut ::core::option::Option<RpcType>,
                tag: u32,
                wire_type: ::prost::encoding::wire_type::WireType,
                buf: &mut impl ::prost::bytes::Buf,
                ctx: ::prost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::prost::DecodeError> {
                match tag {
                    6u32 => {
                        match field {
                            ::core::option::Option::Some(RpcType::Req(ref mut value)) => {
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RpcType::Req(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    7u32 => {
                        match field {
                            ::core::option::Option::Some(RpcType::Rsp(ref mut value)) => {
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RpcType::Rsp(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    8u32 => {
                        match field {
                            ::core::option::Option::Some(RpcType::Ntf(ref mut value)) => {
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RpcType::Ntf(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    _ => {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "internal error: entered unreachable code: {0}",
                                format_args!("invalid RpcType tag: {0}", tag),
                            ),
                        );
                    }
                }
            }
            /// Returns the encoded length of the message without a length delimiter.
            #[inline]
            pub fn encoded_len(&self) -> usize {
                match *self {
                    RpcType::Req(ref value) => {
                        ::prost::encoding::uint32::encoded_len(6u32, &*value)
                    }
                    RpcType::Rsp(ref value) => {
                        ::prost::encoding::uint32::encoded_len(7u32, &*value)
                    }
                    RpcType::Ntf(ref value) => {
                        ::prost::encoding::uint32::encoded_len(8u32, &*value)
                    }
                }
            }
        }
        impl ::core::fmt::Debug for RpcType {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match *self {
                    RpcType::Req(ref value) => {
                        let wrapper = {
                            #[allow(non_snake_case)]
                            fn ScalarWrapper<T>(v: T) -> T {
                                v
                            }
                            ScalarWrapper(&*value)
                        };
                        f.debug_tuple("Req").field(&wrapper).finish()
                    }
                    RpcType::Rsp(ref value) => {
                        let wrapper = {
                            #[allow(non_snake_case)]
                            fn ScalarWrapper<T>(v: T) -> T {
                                v
                            }
                            ScalarWrapper(&*value)
                        };
                        f.debug_tuple("Rsp").field(&wrapper).finish()
                    }
                    RpcType::Ntf(ref value) => {
                        let wrapper = {
                            #[allow(non_snake_case)]
                            fn ScalarWrapper<T>(v: T) -> T {
                                v
                            }
                            ScalarWrapper(&*value)
                        };
                        f.debug_tuple("Ntf").field(&wrapper).finish()
                    }
                }
            }
        }
        /// route type
        pub enum RouteType {
            /// send pkg to dstIdent
            #[prost(uint32, tag = "10")]
            DstIdent(u32),
            /// send pkg to one of idents that match ident with mask
            #[prost(message, tag = "11")]
            DstRandom(DstMask),
            /// send pkg to all idents that match ident with mask
            #[prost(message, tag = "12")]
            DstBroadcast(DstMask),
            /// send pkg to all idents that match ident with mask
            #[prost(message, tag = "13")]
            DstMulticast(DstMulticast),
        }
        #[automatically_derived]
        impl ::core::clone::Clone for RouteType {
            #[inline]
            fn clone(&self) -> RouteType {
                match self {
                    RouteType::DstIdent(__self_0) => {
                        RouteType::DstIdent(::core::clone::Clone::clone(__self_0))
                    }
                    RouteType::DstRandom(__self_0) => {
                        RouteType::DstRandom(::core::clone::Clone::clone(__self_0))
                    }
                    RouteType::DstBroadcast(__self_0) => {
                        RouteType::DstBroadcast(::core::clone::Clone::clone(__self_0))
                    }
                    RouteType::DstMulticast(__self_0) => {
                        RouteType::DstMulticast(::core::clone::Clone::clone(__self_0))
                    }
                }
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for RouteType {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for RouteType {
            #[inline]
            fn eq(&self, other: &RouteType) -> bool {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                __self_discr == __arg1_discr
                    && match (self, other) {
                        (
                            RouteType::DstIdent(__self_0),
                            RouteType::DstIdent(__arg1_0),
                        ) => __self_0 == __arg1_0,
                        (
                            RouteType::DstRandom(__self_0),
                            RouteType::DstRandom(__arg1_0),
                        ) => __self_0 == __arg1_0,
                        (
                            RouteType::DstBroadcast(__self_0),
                            RouteType::DstBroadcast(__arg1_0),
                        ) => __self_0 == __arg1_0,
                        (
                            RouteType::DstMulticast(__self_0),
                            RouteType::DstMulticast(__arg1_0),
                        ) => __self_0 == __arg1_0,
                        _ => unsafe { ::core::intrinsics::unreachable() }
                    }
            }
        }
        impl RouteType {
            /// Encodes the message to a buffer.
            pub fn encode(&self, buf: &mut impl ::prost::bytes::BufMut) {
                match *self {
                    RouteType::DstIdent(ref value) => {
                        ::prost::encoding::uint32::encode(10u32, &*value, buf);
                    }
                    RouteType::DstRandom(ref value) => {
                        ::prost::encoding::message::encode(11u32, &*value, buf);
                    }
                    RouteType::DstBroadcast(ref value) => {
                        ::prost::encoding::message::encode(12u32, &*value, buf);
                    }
                    RouteType::DstMulticast(ref value) => {
                        ::prost::encoding::message::encode(13u32, &*value, buf);
                    }
                }
            }
            /// Decodes an instance of the message from a buffer, and merges it into self.
            pub fn merge(
                field: &mut ::core::option::Option<RouteType>,
                tag: u32,
                wire_type: ::prost::encoding::wire_type::WireType,
                buf: &mut impl ::prost::bytes::Buf,
                ctx: ::prost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::prost::DecodeError> {
                match tag {
                    10u32 => {
                        match field {
                            ::core::option::Option::Some(
                                RouteType::DstIdent(ref mut value),
                            ) => {
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::uint32::merge(wire_type, value, buf, ctx)
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RouteType::DstIdent(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    11u32 => {
                        match field {
                            ::core::option::Option::Some(
                                RouteType::DstRandom(ref mut value),
                            ) => {
                                ::prost::encoding::message::merge(
                                    wire_type,
                                    value,
                                    buf,
                                    ctx,
                                )
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::message::merge(
                                        wire_type,
                                        value,
                                        buf,
                                        ctx,
                                    )
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RouteType::DstRandom(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    12u32 => {
                        match field {
                            ::core::option::Option::Some(
                                RouteType::DstBroadcast(ref mut value),
                            ) => {
                                ::prost::encoding::message::merge(
                                    wire_type,
                                    value,
                                    buf,
                                    ctx,
                                )
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::message::merge(
                                        wire_type,
                                        value,
                                        buf,
                                        ctx,
                                    )
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RouteType::DstBroadcast(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    13u32 => {
                        match field {
                            ::core::option::Option::Some(
                                RouteType::DstMulticast(ref mut value),
                            ) => {
                                ::prost::encoding::message::merge(
                                    wire_type,
                                    value,
                                    buf,
                                    ctx,
                                )
                            }
                            _ => {
                                let mut owned_value = ::core::default::Default::default();
                                let value = &mut owned_value;
                                ::prost::encoding::message::merge(
                                        wire_type,
                                        value,
                                        buf,
                                        ctx,
                                    )
                                    .map(|_| {
                                        *field = ::core::option::Option::Some(
                                            RouteType::DstMulticast(owned_value),
                                        );
                                    })
                            }
                        }
                    }
                    _ => {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "internal error: entered unreachable code: {0}",
                                format_args!("invalid RouteType tag: {0}", tag),
                            ),
                        );
                    }
                }
            }
            /// Returns the encoded length of the message without a length delimiter.
            #[inline]
            pub fn encoded_len(&self) -> usize {
                match *self {
                    RouteType::DstIdent(ref value) => {
                        ::prost::encoding::uint32::encoded_len(10u32, &*value)
                    }
                    RouteType::DstRandom(ref value) => {
                        ::prost::encoding::message::encoded_len(11u32, &*value)
                    }
                    RouteType::DstBroadcast(ref value) => {
                        ::prost::encoding::message::encoded_len(12u32, &*value)
                    }
                    RouteType::DstMulticast(ref value) => {
                        ::prost::encoding::message::encoded_len(13u32, &*value)
                    }
                }
            }
        }
        impl ::core::fmt::Debug for RouteType {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                match *self {
                    RouteType::DstIdent(ref value) => {
                        let wrapper = {
                            #[allow(non_snake_case)]
                            fn ScalarWrapper<T>(v: T) -> T {
                                v
                            }
                            ScalarWrapper(&*value)
                        };
                        f.debug_tuple("DstIdent").field(&wrapper).finish()
                    }
                    RouteType::DstRandom(ref value) => {
                        let wrapper = &*value;
                        f.debug_tuple("DstRandom").field(&wrapper).finish()
                    }
                    RouteType::DstBroadcast(ref value) => {
                        let wrapper = &*value;
                        f.debug_tuple("DstBroadcast").field(&wrapper).finish()
                    }
                    RouteType::DstMulticast(ref value) => {
                        let wrapper = &*value;
                        f.debug_tuple("DstMulticast").field(&wrapper).finish()
                    }
                }
            }
        }
    }
    #[repr(i32)]
    pub enum RetCode {
        RetOk = 0,
        /// Can not find route to destination
        RetUnreachable = 1,
        /// request timeout
        RetTimeout = 2,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for RetCode {
        #[inline]
        fn clone(&self) -> RetCode {
            *self
        }
    }
    #[automatically_derived]
    impl ::core::marker::Copy for RetCode {}
    #[automatically_derived]
    impl ::core::fmt::Debug for RetCode {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::write_str(
                f,
                match self {
                    RetCode::RetOk => "RetOk",
                    RetCode::RetUnreachable => "RetUnreachable",
                    RetCode::RetTimeout => "RetTimeout",
                },
            )
        }
    }
    #[automatically_derived]
    impl ::core::marker::StructuralPartialEq for RetCode {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for RetCode {
        #[inline]
        fn eq(&self, other: &RetCode) -> bool {
            let __self_discr = ::core::intrinsics::discriminant_value(self);
            let __arg1_discr = ::core::intrinsics::discriminant_value(other);
            __self_discr == __arg1_discr
        }
    }
    #[automatically_derived]
    impl ::core::cmp::Eq for RetCode {
        #[inline]
        #[doc(hidden)]
        #[coverage(off)]
        fn assert_receiver_is_total_eq(&self) -> () {}
    }
    #[automatically_derived]
    impl ::core::hash::Hash for RetCode {
        #[inline]
        fn hash<__H: ::core::hash::Hasher>(&self, state: &mut __H) -> () {
            let __self_discr = ::core::intrinsics::discriminant_value(self);
            ::core::hash::Hash::hash(&__self_discr, state)
        }
    }
    #[automatically_derived]
    impl ::core::cmp::PartialOrd for RetCode {
        #[inline]
        fn partial_cmp(
            &self,
            other: &RetCode,
        ) -> ::core::option::Option<::core::cmp::Ordering> {
            let __self_discr = ::core::intrinsics::discriminant_value(self);
            let __arg1_discr = ::core::intrinsics::discriminant_value(other);
            ::core::cmp::PartialOrd::partial_cmp(&__self_discr, &__arg1_discr)
        }
    }
    #[automatically_derived]
    impl ::core::cmp::Ord for RetCode {
        #[inline]
        fn cmp(&self, other: &RetCode) -> ::core::cmp::Ordering {
            let __self_discr = ::core::intrinsics::discriminant_value(self);
            let __arg1_discr = ::core::intrinsics::discriminant_value(other);
            ::core::cmp::Ord::cmp(&__self_discr, &__arg1_discr)
        }
    }
    impl RetCode {
        ///Returns `true` if `value` is a variant of `RetCode`.
        pub fn is_valid(value: i32) -> bool {
            match value {
                0 => true,
                1 => true,
                2 => true,
                _ => false,
            }
        }
        #[deprecated = "Use the TryFrom<i32> implementation instead"]
        ///Converts an `i32` to a `RetCode`, or `None` if `value` is not a valid variant.
        pub fn from_i32(value: i32) -> ::core::option::Option<RetCode> {
            match value {
                0 => ::core::option::Option::Some(RetCode::RetOk),
                1 => ::core::option::Option::Some(RetCode::RetUnreachable),
                2 => ::core::option::Option::Some(RetCode::RetTimeout),
                _ => ::core::option::Option::None,
            }
        }
    }
    impl ::core::default::Default for RetCode {
        fn default() -> RetCode {
            RetCode::RetOk
        }
    }
    impl ::core::convert::From<RetCode> for i32 {
        fn from(value: RetCode) -> i32 {
            value as i32
        }
    }
    impl ::core::convert::TryFrom<i32> for RetCode {
        type Error = ::prost::UnknownEnumValue;
        fn try_from(
            value: i32,
        ) -> ::core::result::Result<RetCode, ::prost::UnknownEnumValue> {
            match value {
                0 => ::core::result::Result::Ok(RetCode::RetOk),
                1 => ::core::result::Result::Ok(RetCode::RetUnreachable),
                2 => ::core::result::Result::Ok(RetCode::RetTimeout),
                _ => ::core::result::Result::Err(::prost::UnknownEnumValue(value)),
            }
        }
    }
    impl RetCode {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::RetOk => "RET_OK",
                Self::RetUnreachable => "RET_UNREACHABLE",
                Self::RetTimeout => "RET_TIMEOUT",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "RET_OK" => Some(Self::RetOk),
                "RET_UNREACHABLE" => Some(Self::RetUnreachable),
                "RET_TIMEOUT" => Some(Self::RetTimeout),
                _ => None,
            }
        }
    }
    impl Hdr {
        pub fn seq(&self) -> Option<u32> {
            use hdr::RpcType;
            match self.rpc_type {
                Some(RpcType::Req(seq)) => Some(seq),
                Some(RpcType::Rsp(seq)) => Some(seq),
                Some(RpcType::Ntf(seq)) => Some(seq),
                _ => None,
            }
        }
        pub fn is_req(&self) -> bool {
            match self.rpc_type {
                Some(hdr::RpcType::Req(_)) => true,
                _ => false,
            }
        }
        pub fn is_rsp(&self) -> bool {
            match self.rpc_type {
                Some(hdr::RpcType::Rsp(_)) => true,
                _ => false,
            }
        }
        pub fn is_ntf(&self) -> bool {
            match self.rpc_type {
                Some(hdr::RpcType::Ntf(_)) => true,
                _ => false,
            }
        }
    }
}
