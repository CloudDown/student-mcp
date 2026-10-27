mod config;
mod http;
mod mcp;
mod pdf;
mod services;
mod session;
mod share;
mod stdio;
mod tray;
mod tunnel;
mod web;

use services::moodle::catalog;
use services::{brightspace, moodle, omnivox};

pub use config::VERSION;
pub use stdio::run as run_stdio;
pub use web::run as run_web;
