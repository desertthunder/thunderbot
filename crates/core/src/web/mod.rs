mod auth;
mod handlers;
mod server;
mod templates;

pub use auth::auth_middleware;
pub use handlers::{DashboardStats, WebAppState};
pub use server::Server;
