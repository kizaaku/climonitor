pub mod cli_tool;
pub mod config;
pub mod message_conversion;
pub mod protocol;
pub mod transport;
pub mod ip_utils;

// 具体的なトランスポート実装
#[cfg(unix)]
pub mod unix_transport;
pub mod grpc_transport;

// gRPC generated code
pub mod grpc {
    tonic::include_proto!("climonitor");
}

pub use cli_tool::*;
pub use config::*;
pub use protocol::*;
pub use transport::*;
