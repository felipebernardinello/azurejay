use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<IpAddr, Window>>>,
    max: u32,
    window: Duration,
}

struct Window {
    start: Instant,
    count: u32,
}

impl RateLimiter {
    #[must_use]
    pub fn new(max: u32, window: Duration) -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())), max, window }
    }

    fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("rate limiter mutex poisoned");
        let entry = map.entry(ip).or_insert(Window { start: now, count: 0 });
        if now.duration_since(entry.start) > self.window {
            entry.start = now;
            entry.count = 0;
        }
        entry.count += 1;
        entry.count <= self.max
    }
}

pub async fn rate_limit_register(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>()
        && !state.register_limiter.check(addr.ip())
    {
        return Err(AppError::TooManyRequests);
    }
    Ok(next.run(req).await)
}
