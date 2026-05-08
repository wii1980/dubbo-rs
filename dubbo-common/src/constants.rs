pub const DUBBO_PROTOCOL: &str = "dubbo";
pub const TRIPLE_PROTOCOL: &str = "tri";
pub const GRPC_PROTOCOL: &str = "grpc";
pub const JSONRPC_PROTOCOL: &str = "jsonrpc";
pub const REST_PROTOCOL: &str = "rest";

pub const ZOOKEEPER_REGISTRY: &str = "zookeeper";
pub const NACOS_REGISTRY: &str = "nacos";
pub const ETCD_REGISTRY: &str = "etcd";

pub const RANDOM_LOADBALANCE: &str = "random";
pub const ROUNDROBIN_LOADBALANCE: &str = "roundrobin";
pub const LEASTACTIVE_LOADBALANCE: &str = "leastactive";
pub const CONSISTENTHASH_LOADBALANCE: &str = "consistenthash";

pub const FAILOVER_CLUSTER: &str = "failover";
pub const FAILFAST_CLUSTER: &str = "failfast";
pub const FAILSAFE_CLUSTER: &str = "failsafe";
pub const FAILBACK_CLUSTER: &str = "failback";
pub const FORKING_CLUSTER: &str = "forking";
pub const BROADCAST_CLUSTER: &str = "broadcast";
pub const AVAILABLE_CLUSTER: &str = "available";

pub const HESSIAN2_SERIALIZATION: &str = "hessian2";
pub const PROTOBUF_SERIALIZATION: &str = "protobuf";
pub const JSON_SERIALIZATION: &str = "json";
pub const MSGPACK_SERIALIZATION: &str = "msgpack";

pub const HESSIAN2_SERIALIZATION_ID: u8 = 2;
pub const PROTOBUF_SERIALIZATION_ID: u8 = 12;

pub const OK_STATUS: u8 = 20;
pub const CLIENT_TIMEOUT_STATUS: u8 = 30;
pub const SERVER_TIMEOUT_STATUS: u8 = 31;
pub const BAD_REQUEST_STATUS: u8 = 40;
pub const BAD_RESPONSE_STATUS: u8 = 50;
pub const SERVICE_NOT_FOUND_STATUS: u8 = 60;
pub const SERVICE_ERROR_STATUS: u8 = 70;
pub const SERVER_ERROR_STATUS: u8 = 80;
pub const CLIENT_ERROR_STATUS: u8 = 90;
pub const SERVER_THREADPOOL_EXHAUSTED: u8 = 100;

pub const DUBBO_MAGIC_HIGH: u8 = 0xda;
pub const DUBBO_MAGIC_LOW: u8 = 0xbb;
pub const DUBBO_HEADER_LENGTH: usize = 16;

pub const FLAG_REQUEST: u8 = 0x80;
pub const FLAG_TWOWAY: u8 = 0x40;
pub const FLAG_EVENT: u8 = 0x20;

pub const ANYHOST_VALUE: &str = "0.0.0.0";
pub const ANYHOST_KEY: &str = "anyhost";
pub const APPLICATION_KEY: &str = "application";
pub const INTERFACE_KEY: &str = "interface";
pub const METHODS_KEY: &str = "methods";
pub const SIDE_KEY: &str = "side";
pub const PROVIDER_SIDE: &str = "provider";
pub const CONSUMER_SIDE: &str = "consumer";

pub const REGISTRY_PROTOCOL_KEY: &str = "registry";

// Generic invocation constants
pub const GENERIC_KEY: &str = "generic";
pub const GENERIC_SERIALIZATION_DEFAULT: &str = "true";
pub const GENERIC_SERIALIZATION_PROTOBUF_JSON: &str = "protobuf-json";
pub const GENERIC_SERIALIZATION_BEAN: &str = "bean";
pub const GENERIC_SERIALIZATION_NATIVE: &str = "native";
pub const RETURN_TYPE_KEY: &str = "return.type";
