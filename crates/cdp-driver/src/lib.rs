pub mod avatar_resource;
pub mod bound_page;
mod page_ops;
pub mod task_page;

pub use bound_page::BoundPage;
pub use task_page::{SelectorState, TaskCancel, TaskError, TaskPage, TaskResult};

pub mod a11y;
pub mod bootstrap;
pub mod safe_cdp;
pub mod scripts;
pub mod session;
pub mod tools;
