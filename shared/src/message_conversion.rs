use crate::grpc::{
    launcher_message, ConnectRequest, ContextUpdate as GrpcContextUpdate, DisconnectRequest,
    LauncherMessage, StateUpdate as GrpcStateUpdate,
};
use crate::{CliToolType, LauncherToMonitor, SessionStatus};
use anyhow::Result;
use chrono::{DateTime, Utc};

/// gRPC メッセージの変換ユーティリティ
pub mod grpc_conversion {
    use super::*;

    /// CliToolType を gRPC の i32 値に変換
    pub fn cli_tool_type_to_grpc(tool_type: CliToolType) -> i32 {
        match tool_type {
            CliToolType::Claude => 0,
            CliToolType::Gemini => 1,
        }
    }

    /// gRPC の i32 値を CliToolType に変換
    pub fn cli_tool_type_from_grpc(value: i32) -> CliToolType {
        match value {
            1 => CliToolType::Gemini,
            _ => CliToolType::Claude,
        }
    }

    /// SessionStatus を gRPC の i32 値に変換
    pub fn session_status_to_grpc(status: SessionStatus) -> i32 {
        match status {
            SessionStatus::Connected => 0,
            SessionStatus::Busy => 1,
            SessionStatus::WaitingInput => 2,
            SessionStatus::Idle => 3,
            SessionStatus::Error => 4,
        }
    }

    /// gRPC の i32 値を SessionStatus に変換
    pub fn session_status_from_grpc(value: i32) -> SessionStatus {
        match value {
            1 => SessionStatus::Busy,
            2 => SessionStatus::WaitingInput,
            3 => SessionStatus::Idle,
            4 => SessionStatus::Error,
            _ => SessionStatus::Connected,
        }
    }

    /// DateTime<Utc> を gRPC Timestamp に変換
    pub fn to_grpc_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
        prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }
    }

    /// gRPC Timestamp を DateTime<Utc> に変換
    pub fn from_grpc_timestamp(ts: Option<prost_types::Timestamp>) -> DateTime<Utc> {
        ts.and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32))
            .unwrap_or_else(Utc::now)
    }

    /// LauncherToMonitor を gRPC LauncherMessage に変換
    pub fn to_grpc_launcher_message(message: LauncherToMonitor) -> Result<LauncherMessage> {
        let grpc_msg = match message {
            LauncherToMonitor::Connect {
                launcher_id,
                project,
                tool_type,
                claude_args,
                working_dir,
                timestamp,
            } => LauncherMessage {
                message: Some(launcher_message::Message::Connect(ConnectRequest {
                    launcher_id,
                    project,
                    tool_type: cli_tool_type_to_grpc(tool_type),
                    claude_args,
                    working_dir: working_dir.to_string_lossy().to_string(),
                    timestamp: Some(to_grpc_timestamp(timestamp)),
                })),
            },

            LauncherToMonitor::StateUpdate {
                launcher_id,
                session_id,
                status,
                ui_above_text,
                timestamp,
            } => LauncherMessage {
                message: Some(launcher_message::Message::StateUpdate(GrpcStateUpdate {
                    launcher_id,
                    session_id,
                    status: session_status_to_grpc(status),
                    ui_above_text,
                    timestamp: Some(to_grpc_timestamp(timestamp)),
                })),
            },

            LauncherToMonitor::ContextUpdate {
                launcher_id,
                session_id,
                ui_above_text,
                timestamp,
            } => LauncherMessage {
                message: Some(launcher_message::Message::ContextUpdate(
                    GrpcContextUpdate {
                        launcher_id,
                        session_id,
                        ui_above_text,
                        timestamp: Some(to_grpc_timestamp(timestamp)),
                    },
                )),
            },

            LauncherToMonitor::Disconnect {
                launcher_id,
                timestamp,
            } => LauncherMessage {
                message: Some(launcher_message::Message::Disconnect(DisconnectRequest {
                    launcher_id,
                    timestamp: Some(to_grpc_timestamp(timestamp)),
                })),
            },
        };

        Ok(grpc_msg)
    }

    /// gRPC LauncherMessage を LauncherToMonitor に変換
    pub fn from_grpc_launcher_message(msg: LauncherMessage) -> Result<LauncherToMonitor> {
        let message = msg
            .message
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

        let protocol_msg = match message {
            launcher_message::Message::Connect(connect_req) => LauncherToMonitor::Connect {
                launcher_id: connect_req.launcher_id,
                project: connect_req.project,
                tool_type: cli_tool_type_from_grpc(connect_req.tool_type),
                claude_args: connect_req.claude_args,
                working_dir: std::path::PathBuf::from(connect_req.working_dir),
                timestamp: from_grpc_timestamp(connect_req.timestamp),
            },

            launcher_message::Message::StateUpdate(state_update) => {
                LauncherToMonitor::StateUpdate {
                    launcher_id: state_update.launcher_id,
                    session_id: state_update.session_id,
                    status: session_status_from_grpc(state_update.status),
                    ui_above_text: state_update.ui_above_text,
                    timestamp: from_grpc_timestamp(state_update.timestamp),
                }
            }

            launcher_message::Message::ContextUpdate(context_update) => {
                LauncherToMonitor::ContextUpdate {
                    launcher_id: context_update.launcher_id,
                    session_id: context_update.session_id,
                    ui_above_text: context_update.ui_above_text,
                    timestamp: from_grpc_timestamp(context_update.timestamp),
                }
            }

            launcher_message::Message::Disconnect(disconnect_req) => {
                LauncherToMonitor::Disconnect {
                    launcher_id: disconnect_req.launcher_id,
                    timestamp: from_grpc_timestamp(disconnect_req.timestamp),
                }
            }
        };

        Ok(protocol_msg)
    }
}
