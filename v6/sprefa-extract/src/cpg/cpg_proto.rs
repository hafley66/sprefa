// HAND-TRANSCRIBED, not prost-build output; see proto/cpg.proto's header.
// Field/oneof numbers read verbatim off codepropertygraph-protos_3:1.7.36.

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CpgStruct {
    #[prost(message, repeated, tag = "1")]
    pub node: ::prost::alloc::vec::Vec<cpg_struct::Node>,
    #[prost(message, repeated, tag = "2")]
    pub edge: ::prost::alloc::vec::Vec<cpg_struct::Edge>,
}

/// Nested-message namespace mirroring `CpgStruct.Node` / `CpgStruct.Edge`.
pub mod cpg_struct {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Property {
        #[prost(int32, tag = "1")]
        pub name: i32,
        #[prost(message, optional, tag = "2")]
        pub value: ::core::option::Option<super::PropertyValue>,
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Node {
        #[prost(int64, tag = "1")]
        pub key: i64,
        #[prost(int32, tag = "2")]
        pub r#type: i32,
        #[prost(message, repeated, tag = "3")]
        pub property: ::prost::alloc::vec::Vec<Property>,
    }

    /// `CpgStruct.Edge.Property` is a distinct message from `Node.Property`
    /// (separate EdgePropertyName id space); kept as its own type below.
    pub mod edge {
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Property {
            #[prost(int32, tag = "1")]
            pub name: i32,
            #[prost(message, optional, tag = "2")]
            pub value: ::core::option::Option<super::super::PropertyValue>,
        }
    }

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Edge {
        #[prost(int64, tag = "1")]
        pub src: i64,
        #[prost(int64, tag = "2")]
        pub dst: i64,
        #[prost(int32, tag = "3")]
        pub r#type: i32,
        #[prost(message, repeated, tag = "4")]
        pub property: ::prost::alloc::vec::Vec<edge::Property>,
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StringList {
    #[prost(string, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BoolList {
    #[prost(bool, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<bool>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IntList {
    #[prost(int32, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<i32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LongList {
    #[prost(int64, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<i64>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatList {
    #[prost(float, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<f32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DoubleList {
    #[prost(double, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<f64>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainedRefs {
    #[prost(string, tag = "1")]
    pub local_name: ::prost::alloc::string::String,
    #[prost(int64, repeated, tag = "2")]
    pub refs: ::prost::alloc::vec::Vec<i64>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PropertyValue {
    #[prost(
        oneof = "property_value::Value",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13"
    )]
    pub value: ::core::option::Option<property_value::Value>,
}

pub mod property_value {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(string, tag = "1")]
        StringValue(::prost::alloc::string::String),
        #[prost(bool, tag = "2")]
        BoolValue(bool),
        #[prost(int32, tag = "3")]
        IntValue(i32),
        #[prost(int64, tag = "4")]
        LongValue(i64),
        #[prost(float, tag = "5")]
        FloatValue(f32),
        #[prost(double, tag = "6")]
        DoubleValue(f64),
        #[prost(message, tag = "7")]
        StringList(super::StringList),
        #[prost(message, tag = "8")]
        BoolList(super::BoolList),
        #[prost(message, tag = "9")]
        IntList(super::IntList),
        #[prost(message, tag = "10")]
        LongList(super::LongList),
        #[prost(message, tag = "11")]
        FloatList(super::FloatList),
        #[prost(message, tag = "12")]
        DoubleList(super::DoubleList),
        #[prost(message, tag = "13")]
        ContainedRefs(super::ContainedRefs),
    }
}
