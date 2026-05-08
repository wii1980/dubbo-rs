#[cfg(any(
    all(feature = "configcenter-zookeeper", feature = "configcenter-nacos"),
    all(feature = "configcenter-zookeeper", feature = "configcenter-apollo"),
    all(feature = "configcenter-nacos", feature = "configcenter-apollo"),
))]
compile_error!(
    "The features `configcenter-zookeeper`, `configcenter-nacos`, and `configcenter-apollo` are mutually exclusive. Enable only one config center backend at a time."
);

mod instance;

pub use dubbo_rs_cluster as cluster;
pub use dubbo_rs_common;
pub use dubbo_rs_common as common;
pub use dubbo_rs_config as config;
pub use dubbo_rs_filter as filter;
pub use dubbo_rs_loadbalance as loadbalance;
pub use dubbo_rs_logger as logger;
pub use dubbo_rs_macros;
pub use dubbo_rs_protocol as protocol;
pub use dubbo_rs_proxy as proxy;
pub use dubbo_rs_registry as registry;
pub use dubbo_rs_remoting as remoting;
pub use dubbo_rs_serialization as serialization;
pub use instance::Instance;

#[cfg(feature = "metadata")]
pub use dubbo_rs_metadata;

#[cfg(feature = "client")]
pub use dubbo_rs_client as client;
#[cfg(feature = "server")]
pub use dubbo_rs_server as server;

#[cfg(feature = "client")]
pub use dubbo_rs_macros::client;
#[cfg(feature = "server")]
pub use dubbo_rs_macros::service;

pub use dubbo_rs_protocol_triple as triple;

#[cfg(feature = "dubbo2")]
pub use dubbo_rs_protocol_dubbo as dubbo2;

#[cfg(feature = "grpc")]
pub use dubbo_rs_protocol_grpc as grpc;

#[cfg(feature = "jsonrpc")]
pub use dubbo_rs_protocol_jsonrpc as jsonrpc;

#[cfg(feature = "rest")]
pub use dubbo_rs_protocol_rest as rest;

#[cfg(feature = "zookeeper")]
pub use dubbo_rs_registry_zookeeper as zookeeper;

#[cfg(feature = "nacos")]
pub use dubbo_rs_registry_nacos as nacos;

#[cfg(feature = "etcd")]
pub use dubbo_rs_registry_etcd as etcd;

#[cfg(any(
    feature = "configcenter-zookeeper",
    feature = "configcenter-nacos",
    feature = "configcenter-apollo",
))]
pub use dubbo_rs_configcenter as configcenter;

#[cfg(feature = "configcenter-zookeeper")]
pub use dubbo_rs_configcenter_zookeeper as configcenter_zookeeper;

#[cfg(feature = "configcenter-nacos")]
pub use dubbo_rs_configcenter_nacos as configcenter_nacos;

#[cfg(feature = "configcenter-apollo")]
pub use dubbo_rs_configcenter_apollo as configcenter_apollo;

pub use dubbo_rs_serialization_protobuf as serialization_protobuf;

#[cfg(feature = "serialization-hessian2")]
pub use dubbo_rs_serialization_hessian2 as serialization_hessian2;

#[cfg(feature = "serialization-json")]
pub use dubbo_rs_serialization_json as serialization_json;

#[cfg(feature = "metrics")]
pub use dubbo_rs_metrics as metrics;

#[cfg(feature = "tracing")]
pub use dubbo_rs_tracing as tracing;

#[cfg(feature = "tls")]
pub use dubbo_rs_tls as tls;

#[cfg(feature = "metadata")]
pub use dubbo_rs_metadata as metadata;
