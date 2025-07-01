use climonitor_shared::SessionStatus;

/// Claude セッションの状態
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// アイドル状態（入力待ち）
    Idle,
    /// ビジー状態（処理中）
    Busy,
    /// ユーザー入力待ち（承認など）
    WaitingForInput,
    /// エラー状態
    Error,
    /// 接続中（初期状態）
    Connected,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Idle => write!(f, "⚪ Idle"),
            SessionState::Busy => write!(f, "🟢 Busy"),
            SessionState::WaitingForInput => write!(f, "⏳ Waiting"),
            SessionState::Error => write!(f, "🔴 Error"),
            SessionState::Connected => write!(f, "🔗 Connected"),
        }
    }
}

impl SessionState {
    /// SessionStateをプロトコル用のSessionStatusに変換
    pub fn to_session_status(&self) -> SessionStatus {
        match self {
            SessionState::Idle => SessionStatus::Idle,
            SessionState::Busy => SessionStatus::Busy,
            SessionState::WaitingForInput => SessionStatus::WaitingInput,
            SessionState::Error => SessionStatus::Error,
            SessionState::Connected => SessionStatus::Idle, // Connectedは一時的なのでIdleとして扱う
        }
    }
}
