pub mod error;
pub mod sender;

// 重新导出常用类型和接口
pub use self::error::{MessageError, handle_message_error};
pub use self::sender::{MessageSender, MessageTarget, DefaultMessageSender};