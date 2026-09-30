//! 管线错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("解析错误: {0}")]
    Parse(String),

    #[error("功能未实现: {0}")]
    NotImplemented(String),

    #[error("配置错误: {0}")]
    Config(String),
}
