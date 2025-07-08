pub mod cli_tool;
pub mod config;
pub mod ip_utils;
pub mod message_conversion;
pub mod protocol;
pub mod transport;

// 具体的なトランスポート実装
pub mod grpc_transport;
#[cfg(unix)]
pub mod unix_transport;

// gRPC generated code
pub mod grpc {
    tonic::include_proto!("climonitor");
}

pub use cli_tool::*;
pub use config::*;
pub use protocol::*;
pub use transport::*;
